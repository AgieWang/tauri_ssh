import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Badge,
  Button,
  Card,
  Divider,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from "antd";
import type { TableProps } from "antd";
import { RefreshCw } from "lucide-react";
import {
  AiInsightPanel,
  CodeBlock,
  PageHeader,
  SectionGrid,
  StatCard,
} from "@/components/prototype/common";
import {
  aiProviderApi,
  approvalApi,
  auditApi,
  getErrorMessage,
  jumpserverApi,
  mcpApi,
  sshServerApi,
} from "@/lib/api";
import type {
  AiProvider,
  ApprovalRequest,
  AuditLog,
  JumpServerSession,
  McpOverview,
  SshServer,
  SshServerPolicy,
} from "@/types";

const { Paragraph, Text } = Typography;

const sshServerStatusMeta: Record<
  SshServer["status"],
  {
    text: string;
    status: "success" | "processing" | "default" | "warning" | "error";
  }
> = {
  unknown: { text: "未检测", status: "default" },
  online: { text: "在线", status: "success" },
  offline: { text: "离线", status: "error" },
  degraded: { text: "异常", status: "warning" },
  web: { text: "网页登录", status: "processing" },
};

const sshPolicyLabel: Record<SshServerPolicy, string> = {
  readonly: "只读 - 仅允许查看",
  L1: "低风险 - 只读与安全检查",
  L2: "中风险 - 常规运维需审批",
  L3: "高风险 - 变更/重启强审批",
  blocked: "禁用 - AI 不可操作",
};

const sshPolicyColor: Record<SshServerPolicy, string> = {
  readonly: "cyan",
  L1: "green",
  L2: "orange",
  L3: "red",
  blocked: "red",
};

function isConfiguredProvider(provider: AiProvider) {
  return provider.enabled && provider.status === "configured";
}

function formatDashboardTime(value: string | null | undefined) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function approvalRiskTag(risk: string) {
  const riskColorMap: Record<string, string> = {
    readonly: "cyan",
    L1: "green",
    L2: "orange",
    L3: "red",
    review: "gold",
    high: "red",
    blocked: "red",
  };
  return <Tag color={riskColorMap[risk] ?? "default"}>{risk}</Tag>;
}

function scheduleDeferredLoad(callback: () => void) {
  const timer = window.setTimeout(callback, 300);
  return () => window.clearTimeout(timer);
}

export default function DashboardPage() {
  const navigate = useNavigate();
  const [loadingCore, setLoadingCore] = useState(false);
  const [loadingDeferred, setLoadingDeferred] = useState(false);
  const [dashboardServers, setDashboardServers] = useState<SshServer[]>([]);
  const [dashboardApprovals, setDashboardApprovals] = useState<
    ApprovalRequest[]
  >([]);
  const [dashboardAudits, setDashboardAudits] = useState<AuditLog[]>([]);
  const [dashboardProviders, setDashboardProviders] = useState<AiProvider[]>(
    [],
  );
  const [dashboardJumpSessions, setDashboardJumpSessions] = useState<
    JumpServerSession[]
  >([]);
  const [dashboardMcp, setDashboardMcp] = useState<McpOverview | null>(null);
  const coreRequestId = useRef(0);
  const deferredRequestId = useRef(0);

  const loadCore = useCallback(async () => {
    const requestId = ++coreRequestId.current;
    setLoadingCore(true);
    try {
      const [serverResult, approvalResult, providerResult] =
        await Promise.allSettled([
          sshServerApi.list(),
          approvalApi.list({ status: "pending", limit: 20 }),
          aiProviderApi.list(),
        ]);

      if (requestId !== coreRequestId.current) return;
      setDashboardServers(
        serverResult.status === "fulfilled" ? serverResult.value : [],
      );
      setDashboardApprovals(
        approvalResult.status === "fulfilled" ? approvalResult.value : [],
      );
      setDashboardProviders(
        providerResult.status === "fulfilled" ? providerResult.value : [],
      );

      const firstRejected = [serverResult, approvalResult, providerResult].find(
        (item) => item.status === "rejected",
      );
      if (firstRejected?.status === "rejected") {
        message.warning(
          `部分工作台数据加载失败：${getErrorMessage(firstRejected.reason)}`,
        );
      }
    } finally {
      if (requestId === coreRequestId.current) {
        setLoadingCore(false);
      }
    }
  }, []);

  const loadDeferred = useCallback(async () => {
    const requestId = ++deferredRequestId.current;
    setLoadingDeferred(true);
    try {
      const [auditResult, jumpserverResult, mcpResult] =
        await Promise.allSettled([
          auditApi.list({ limit: 10 }),
          jumpserverApi.list(),
          mcpApi.overview(),
        ]);
      if (requestId !== deferredRequestId.current) return;
      setDashboardAudits(
        auditResult.status === "fulfilled" ? auditResult.value : [],
      );
      setDashboardJumpSessions(
        jumpserverResult.status === "fulfilled" ? jumpserverResult.value : [],
      );
      setDashboardMcp(
        mcpResult.status === "fulfilled" ? mcpResult.value : null,
      );
    } finally {
      if (requestId === deferredRequestId.current) {
        setLoadingDeferred(false);
      }
    }
  }, []);

  const refreshDashboard = useCallback(async () => {
    await Promise.all([loadCore(), loadDeferred()]);
  }, [loadCore, loadDeferred]);

  useEffect(() => {
    void loadCore();
    const cancelDeferredLoad = scheduleDeferredLoad(() => void loadDeferred());
    return () => {
      cancelDeferredLoad();
      coreRequestId.current += 1;
      deferredRequestId.current += 1;
    };
  }, [loadCore, loadDeferred]);

  const recentServers = useMemo(
    () =>
      [...dashboardServers]
        .sort((left, right) => {
          const leftTime = new Date(
            left.lastConnectedAt ?? left.updatedAt,
          ).getTime();
          const rightTime = new Date(
            right.lastConnectedAt ?? right.updatedAt,
          ).getTime();
          return rightTime - leftTime;
        })
        .slice(0, 6),
    [dashboardServers],
  );
  const enabledServers = dashboardServers.filter((item) => item.enabled);
  const onlineServers = dashboardServers.filter(
    (item) => item.status === "online",
  );
  const configuredProviders = dashboardProviders.filter(isConfiguredProvider);
  const activeJumpSessions = dashboardJumpSessions.filter(
    (item) => item.enabled && item.status !== "disabled",
  );

  const dashboardStats = [
    {
      label: "服务器资产",
      value: String(dashboardServers.length),
      hint: `${enabledServers.length} 台启用，${onlineServers.length} 台在线`,
    },
    {
      label: "待审批请求",
      value: String(dashboardApprovals.length),
      hint:
        dashboardApprovals.length > 0
          ? "存在需人工确认的 AI 操作"
          : "暂无待处理审批",
    },
    {
      label: "近期审计",
      value: String(dashboardAudits.length),
      hint: "展示最近 10 条操作记录",
    },
    {
      label: "AI Provider",
      value: String(configuredProviders.length),
      hint: `共 ${dashboardProviders.length} 个 Provider，MCP 工具 ${dashboardMcp?.tools.length ?? 0} 个`,
    },
  ];

  const dashboardServerColumns: TableProps<SshServer>["columns"] = [
    { title: "别名", dataIndex: "alias" },
    { title: "分组", dataIndex: "groupName" },
    {
      title: "地址",
      render: (_, record) => `${record.username}@${record.host}:${record.port}`,
    },
    {
      title: "AI 权限",
      dataIndex: "aiPolicy",
      render: (value: SshServerPolicy) => (
        <Tag color={sshPolicyColor[value]}>{sshPolicyLabel[value]}</Tag>
      ),
    },
    {
      title: "状态",
      dataIndex: "status",
      render: (value: SshServer["status"]) => {
        const meta = sshServerStatusMeta[value] ?? sshServerStatusMeta.unknown;
        return <Badge status={meta.status} text={meta.text} />;
      },
    },
    {
      title: "最近连接",
      dataIndex: "lastConnectedAt",
      render: (value: string | null) => formatDashboardTime(value),
    },
    {
      title: "操作",
      width: 110,
      render: (_, record) => (
        <Button
          size="small"
          type="primary"
          disabled={!record.enabled}
          onClick={() =>
            navigate(
              `/terminal?server=${encodeURIComponent(record.alias)}&connect=1&source=dashboard&request=${Date.now()}`,
            )
          }
        >
          连接
        </Button>
      ),
    },
  ];

  const dashboardApprovalColumns: TableProps<ApprovalRequest>["columns"] = [
    { title: "编号", dataIndex: "id", width: 80 },
    { title: "来源", dataIndex: "source", width: 110 },
    { title: "服务器", dataIndex: "serverAlias", width: 150 },
    { title: "动作", dataIndex: "action", width: 140 },
    {
      title: "风险",
      dataIndex: "risk",
      width: 90,
      render: (value: string) => approvalRiskTag(value),
    },
    { title: "摘要", dataIndex: "summary", ellipsis: true },
    {
      title: "创建时间",
      dataIndex: "createdAt",
      width: 130,
      render: (value: string) => formatDashboardTime(value),
    },
  ];

  const dashboardAuditColumns: TableProps<AuditLog>["columns"] = [
    {
      title: "时间",
      dataIndex: "occurredAt",
      width: 130,
      render: (value: string) => formatDashboardTime(value),
    },
    {
      title: "来源",
      dataIndex: "source",
      width: 170,
      render: (value: string) => (
        <Text style={{ whiteSpace: "nowrap" }}>{value}</Text>
      ),
    },
    {
      title: "服务器",
      dataIndex: "serverAlias",
      width: 150,
      render: (value: string) => value || "-",
    },
    { title: "动作", dataIndex: "action", width: 207 },
    {
      title: "结果",
      dataIndex: "result",
      width: 80,
      render: (value: string) => (
        <Tag
          color={
            value === "success"
              ? "green"
              : value === "blocked"
                ? "red"
                : "orange"
          }
        >
          {value}
        </Tag>
      ),
    },
    { title: "摘要", dataIndex: "summary", width: 360, ellipsis: true },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="工作台"
        description="集中展示真实服务器资产、待审批请求、近期审计、AI Provider 与 MCP Server 状态。"
        actions={
          <Space>
            <Button
              onClick={() => void refreshDashboard()}
              loading={loadingCore || loadingDeferred}
              icon={<RefreshCw size={14} />}
            >
              刷新
            </Button>
            <Button type="primary" onClick={() => navigate("/servers")}>
              新建 SSH 会话
            </Button>
          </Space>
        }
      />
      <SectionGrid columns={4}>
        {dashboardStats.map((item) => (
          <StatCard key={item.label} {...item} />
        ))}
      </SectionGrid>
      <Card
        title="服务器快捷入口"
        extra={
          <Button size="small" onClick={() => navigate("/servers")}>
            管理服务器
          </Button>
        }
      >
        <Table
          size="small"
          loading={loadingCore}
          pagination={false}
          rowKey="alias"
          columns={dashboardServerColumns}
          dataSource={recentServers}
        />
      </Card>
      <Card
        title="待审批"
        extra={
          <Button size="small" onClick={() => navigate("/approval")}>
            进入审批队列
          </Button>
        }
      >
        <Table
          size="small"
          loading={loadingCore}
          pagination={false}
          rowKey="id"
          columns={dashboardApprovalColumns}
          dataSource={dashboardApprovals}
        />
      </Card>
      <Card
        title="近期审计"
        extra={
          <Button size="small" onClick={() => navigate("/audit")}>
            查看审计日志
          </Button>
        }
      >
        <Table
          size="small"
          loading={loadingDeferred}
          pagination={false}
          rowKey="id"
          columns={dashboardAuditColumns}
          dataSource={dashboardAudits}
        />
      </Card>
      <AiInsightPanel title="运行状态">
        <Space orientation="vertical" size={12} style={{ width: "100%" }}>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">MCP Server</Text>
            {dashboardMcp ? (
              <Badge
                status={
                  dashboardMcp.status.httpReachable ? "success" : "warning"
                }
                text={
                  dashboardMcp.status.httpReachable
                    ? "HTTP 可用"
                    : "本机配置可用"
                }
              />
            ) : (
              <Tag>{loadingDeferred ? "加载中" : "未加载"}</Tag>
            )}
          </div>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">Agent 客户端</Text>
            <Text>
              {dashboardMcp?.clients.filter((item) => item.configured).length ??
                0}
              /{dashboardMcp?.clients.length ?? 0} 已配置
            </Text>
          </div>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">堡垒机会话</Text>
            <Text>{activeJumpSessions.length} 个可用引用</Text>
          </div>
          <Divider style={{ margin: "4px 0" }} />
          <Paragraph style={{ marginBottom: 0 }}>
            工作台数据来自本机 SQLite 与 Tauri 后端
            Command。可从这里快速进入服务器连接、审批处理和审计追踪。
          </Paragraph>
          <CodeBlock style={{ marginBottom: 0 }}>
            {dashboardMcp?.status.streamableHttpUrl ??
              "MCP Server 地址将在后端启动后显示"}
          </CodeBlock>
        </Space>
      </AiInsightPanel>
    </div>
  );
}

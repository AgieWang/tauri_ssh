import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Button,
  Card,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { Plus, RefreshCw } from "lucide-react";
import { getErrorMessage, mcpApi, secureCredentialApi } from "@/lib/api";
import type {
  SecureCredential,
  SecureCredentialAuditLog,
  McpToolPermission,
  SecureCredentialOverview,
  SecureCredentialPolicySettings,
  SecureCredentialProvider,
  UpsertSecureCredentialInput,
  SecureCredentialSession,
} from "@/types";
import { PageHeader } from "@/components/prototype/common";

const { Paragraph, Text } = Typography;

const providerOptions = [
  { label: "GitHub", value: "github" },
  { label: "GitLab", value: "gitlab" },
  { label: "GitCode", value: "gitcode" },
  { label: "Gitee", value: "gitee" },
  { label: "HTTP API", value: "http_api" },
  { label: "自定义", value: "custom" },
];

const defaultScopesByProvider: Record<SecureCredentialProvider, string[]> = {
  github: ["repo:read"],
  gitlab: ["read_repository"],
  gitcode: ["repo:read"],
  gitee: ["repo:read"],
  http_api: ["http:read"],
  custom: ["read"],
};

const scopeOptionsByProvider: Record<
  SecureCredentialProvider,
  { label: string; options: { label: string; value: string }[] }[]
> = {
  github: [
    {
      label: "仓库",
      options: [
        { label: "读取仓库 repo:read", value: "repo:read" },
        { label: "写入仓库 repo:write", value: "repo:write" },
        { label: "仓库管理 repo:admin", value: "repo:admin" },
        { label: "仓库全部权限 repo:all", value: "repo:all" },
      ],
    },
    {
      label: "协作",
      options: [
        { label: "读取 PR pull_request:read", value: "pull_request:read" },
        { label: "写入 PR pull_request:write", value: "pull_request:write" },
        { label: "读取 Issue issue:read", value: "issue:read" },
        { label: "写入 Issue issue:write", value: "issue:write" },
        { label: "读取 Release release:read", value: "release:read" },
        { label: "写入 Release release:write", value: "release:write" },
      ],
    },
    {
      label: "高级",
      options: [
        { label: "分支写入 branch:write", value: "branch:write" },
        { label: "读取 Workflow workflow:read", value: "workflow:read" },
        { label: "写入 Workflow workflow:write", value: "workflow:write" },
        { label: "读取组织 org:read", value: "org:read" },
        { label: "读取用户 user:read", value: "user:read" },
        { label: "全部权限 all", value: "all" },
      ],
    },
  ],
  gitlab: [
    {
      label: "API",
      options: [
        { label: "读取 API read_api", value: "read_api" },
        { label: "完整 API api", value: "api" },
        { label: "sudo sudo", value: "sudo" },
        { label: "全部权限 all", value: "all" },
      ],
    },
    {
      label: "仓库",
      options: [
        { label: "读取仓库 read_repository", value: "read_repository" },
        { label: "写入仓库 write_repository", value: "write_repository" },
        { label: "读取 Registry read_registry", value: "read_registry" },
        { label: "写入 Registry write_registry", value: "write_registry" },
      ],
    },
  ],
  gitcode: [
    {
      label: "仓库",
      options: [
        { label: "读取仓库 repo:read", value: "repo:read" },
        { label: "写入仓库 repo:write", value: "repo:write" },
        { label: "仓库管理 repo:admin", value: "repo:admin" },
      ],
    },
    {
      label: "协作",
      options: [
        { label: "读取 PR pull_request:read", value: "pull_request:read" },
        { label: "写入 PR pull_request:write", value: "pull_request:write" },
        { label: "读取 Issue issue:read", value: "issue:read" },
        { label: "写入 Issue issue:write", value: "issue:write" },
        { label: "写入 Release release:write", value: "release:write" },
        { label: "全部权限 all", value: "all" },
      ],
    },
  ],
  gitee: [
    {
      label: "仓库",
      options: [
        { label: "读取仓库 repo:read", value: "repo:read" },
        { label: "写入仓库 repo:write", value: "repo:write" },
        { label: "仓库管理 repo:admin", value: "repo:admin" },
      ],
    },
    {
      label: "协作",
      options: [
        { label: "读取 PR pull_request:read", value: "pull_request:read" },
        { label: "写入 PR pull_request:write", value: "pull_request:write" },
        { label: "读取 Issue issue:read", value: "issue:read" },
        { label: "写入 Issue issue:write", value: "issue:write" },
        { label: "写入 Release release:write", value: "release:write" },
        { label: "全部权限 all", value: "all" },
      ],
    },
  ],
  http_api: [
    {
      label: "HTTP",
      options: [
        { label: "GET http:get", value: "http:get" },
        { label: "POST http:post", value: "http:post" },
        { label: "PUT http:put", value: "http:put" },
        { label: "PATCH http:patch", value: "http:patch" },
        { label: "DELETE http:delete", value: "http:delete" },
        { label: "只读 http:read", value: "http:read" },
        { label: "写入 http:write", value: "http:write" },
        { label: "全部权限 http:all", value: "http:all" },
      ],
    },
  ],
  custom: [
    {
      label: "通用",
      options: [
        { label: "读取 read", value: "read" },
        { label: "写入 write", value: "write" },
        { label: "管理 admin", value: "admin" },
        { label: "全部权限 all", value: "all" },
      ],
    },
  ],
};

const credentialTypeOptions = [
  { label: "Token", value: "token" },
  { label: "API Key", value: "api_key" },
  { label: "Bearer Token", value: "bearer_token" },
  { label: "Basic Auth", value: "basic_auth" },
  { label: "自定义密钥", value: "custom_secret" },
  { label: "会话引用", value: "session_reference" },
];

const tagPresetOptions = [
  { label: "生产", value: "生产" },
  { label: "测试", value: "测试" },
  { label: "开发", value: "开发" },
  { label: "个人", value: "个人" },
  { label: "团队", value: "团队" },
  { label: "只读", value: "只读" },
  { label: "读写", value: "读写" },
  { label: "管理员", value: "管理员" },
  { label: "Git", value: "Git" },
  { label: "CI/CD", value: "CI/CD" },
  { label: "MCP", value: "MCP" },
  { label: "高风险", value: "高风险" },
];

const approvalPolicyOptions = [
  { label: "只读自动执行", value: "readonly_auto" },
  { label: "写操作需审批", value: "write_requires_approval" },
  { label: "全部操作需审批", value: "all_requires_approval" },
  { label: "禁止 MCP 使用", value: "blocked_for_mcp" },
];

const statusMeta: Record<string, { label: string; color: string }> = {
  active: { label: "正常", color: "green" },
  disabled: { label: "禁用", color: "default" },
  rotation_due: { label: "需轮换", color: "orange" },
  expired: { label: "已过期", color: "red" },
  test_failed: { label: "测试失败", color: "red" },
};

const sessionStatusMeta: Record<string, { label: string; color: string }> = {
  active: { label: "有效", color: "green" },
  expired: { label: "已过期", color: "orange" },
  revoked: { label: "已吊销", color: "red" },
};

function providerLabel(provider: string) {
  return providerOptions.find((item) => item.value === provider)?.label ?? provider;
}

function statusTag(status: string) {
  const meta = statusMeta[status] ?? { label: status, color: "default" };
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

function sessionStatusTag(status: string) {
  const meta = sessionStatusMeta[status] ?? { label: status, color: "default" };
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

function createEmptyOverview(): SecureCredentialOverview {
  return {
    total: 0,
    active: 0,
    disabled: 0,
    mcpEnabled: 0,
    expiringSoon: 0,
    weeklyCalls: 0,
    successRate: 0,
  };
}

function createDefaultPolicySettings(): SecureCredentialPolicySettings {
  return {
    defaultSessionTtlMinutes: 30,
    maxResponseItems: 100,
    allowReadonlyAuto: true,
    requireApprovalForAll: false,
    allowHttpCustomHeaders: false,
    httpAllowedDomains: [],
    rateLimitPerMinute: 60,
    maxConcurrentSessions: 5,
    allowDefaultBranchCommits: false,
    allowHighRiskRepoOps: false,
    allowDeleteBranch: false,
    allowDeleteTag: false,
    allowDeleteRelease: false,
    allowUpdateRef: false,
    allowUpdateRepoSettings: false,
    updatedAt: null,
  };
}

const secureCredentialMcpToolPrefixes = ["secure_", "github_", "gitlab_", "gitcode_", "gitee_", "http_api_"];

function isSecureCredentialMcpTool(toolName: string) {
  return secureCredentialMcpToolPrefixes.some((prefix) => toolName.startsWith(prefix));
}

function OverviewCards({ overview }: { overview: SecureCredentialOverview }) {
  return (
    <div className="prototype-grid prototype-grid-4">
      <Card>
        <Statistic title="凭证总数" value={overview.total} />
      </Card>
      <Card>
        <Statistic title="可用凭证" value={overview.active} />
      </Card>
      <Card>
        <Statistic title="MCP 可用" value={overview.mcpEnabled} />
      </Card>
      <Card>
        <Statistic title="14 天内过期" value={overview.expiringSoon} />
      </Card>
    </div>
  );
}

function useSecureCredentialData() {
  const [overview, setOverview] = useState<SecureCredentialOverview>(createEmptyOverview());
  const [items, setItems] = useState<SecureCredential[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [overviewResult, listResult] = await Promise.all([
        secureCredentialApi.overview(),
        secureCredentialApi.list(),
      ]);
      setOverview(overviewResult);
      setItems(listResult);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return { overview, items, loading, load };
}

function useSecureCredentialSessions() {
  const [sessions, setSessions] = useState<SecureCredentialSession[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setSessions(await secureCredentialApi.listSessions());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return { sessions, loading, load };
}

export function SecureCredentialOverviewPage() {
  const { overview, items, loading, load } = useSecureCredentialData();
  const recentItems = items.slice(0, 6);

  return (
    <div className="prototype-page">
      <PageHeader
        title="安全凭证"
        description="为 AI/MCP 提供受控凭证会话，凭证明文只在本机后端内部使用。"
        actions={
          <Button icon={<RefreshCw size={14} />} onClick={() => void load()} loading={loading}>
            刷新
          </Button>
        }
      />
      <OverviewCards overview={overview} />
      <div className="prototype-grid prototype-grid-2">
        <Card title="最近凭证" loading={loading}>
          <Table
            size="small"
            rowKey="credentialKey"
            dataSource={recentItems}
            pagination={false}
            columns={[
              { title: "Key", dataIndex: "credentialKey" },
              { title: "Provider", dataIndex: "provider", render: providerLabel },
              { title: "状态", dataIndex: "status", render: statusTag },
              { title: "最后更新", dataIndex: "updatedAt" },
            ]}
          />
        </Card>
        <Card title="治理状态">
          <Space direction="vertical" size="middle" style={{ width: "100%" }}>
            <Text>本周调用：{overview.weeklyCalls}</Text>
            <Text>授权成功率：{overview.successRate.toFixed(1)}%</Text>
            <Text>禁用凭证：{overview.disabled}</Text>
            <Paragraph type="secondary" style={{ marginBottom: 0 }}>
              当前阶段已实现本地凭证加密存储和治理页面骨架。会话代理、Provider Adapter 和 MCP 工具按后续阶段继续接入。
            </Paragraph>
          </Space>
        </Card>
      </div>
    </div>
  );
}

export function SecureCredentialVaultPage() {
  const { overview, items, loading, load } = useSecureCredentialData();
  const [form] = Form.useForm<UpsertSecureCredentialInput>();
  const selectedProvider = Form.useWatch("provider", form) as SecureCredentialProvider | undefined;
  const [keyword, setKeyword] = useState("");
  const [provider, setProvider] = useState<SecureCredentialProvider | "">("");
  const [editing, setEditing] = useState<SecureCredential | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [rotating, setRotating] = useState<SecureCredential | null>(null);
  const [rotateSecret, setRotateSecret] = useState("");

  const filteredItems = useMemo(() => {
    const normalized = keyword.trim().toLowerCase();
    return items
      .filter((item) => !provider || item.provider === provider)
      .filter((item) => {
        if (!normalized) return true;
        return [
          item.credentialKey,
          item.displayName,
          item.provider,
          item.accountName,
          item.folder,
          item.description,
          item.tags.join(" "),
        ]
          .join(" ")
          .toLowerCase()
          .includes(normalized);
      });
  }, [items, keyword, provider]);

  const tagOptions = useMemo(() => {
    const values = new Set(tagPresetOptions.map((item) => item.value));
    items.forEach((item) => item.tags.forEach((tag) => values.add(tag)));
    return Array.from(values).map((tag) => ({ label: tag, value: tag }));
  }, [items]);

  function openCreate() {
    setEditing(null);
    form.setFieldsValue({
      credentialKey: "",
      displayName: "",
      provider: "github",
      credentialType: "token",
      accountName: "",
      baseUrl: "",
      scopes: defaultScopesByProvider.github,
      tags: [],
      folder: "",
      description: "",
      status: "active",
      enabled: true,
      allowMcp: false,
      approvalPolicy: "write_requires_approval",
      expiresAt: null,
      secret: "",
    });
    setDrawerOpen(true);
  }

  function openEdit(item: SecureCredential) {
    setEditing(item);
    form.setFieldsValue({
      id: item.id,
      credentialKey: item.credentialKey,
      displayName: item.displayName,
      provider: item.provider,
      credentialType: item.credentialType,
      accountName: item.accountName,
      baseUrl: item.baseUrl,
      scopes: item.scopes,
      tags: item.tags,
      folder: item.folder,
      description: item.description,
      status: item.status,
      enabled: item.enabled,
      allowMcp: item.allowMcp,
      approvalPolicy: item.approvalPolicy,
      expiresAt: item.expiresAt,
      secret: "",
    });
    setDrawerOpen(true);
  }

  async function submit() {
    const values = await form.validateFields();
    setSaving(true);
    try {
      await secureCredentialApi.upsert({
        ...values,
        scopes: values.scopes ?? [],
        tags: values.tags ?? [],
        secret: values.secret?.trim() ? values.secret.trim() : null,
      });
      message.success(editing ? "安全凭证已更新" : "安全凭证已创建");
      setDrawerOpen(false);
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  async function toggleEnabled(item: SecureCredential, enabled: boolean) {
    try {
      await secureCredentialApi.setEnabled({ credentialKey: item.credentialKey, enabled });
      message.success(enabled ? "已启用" : "已禁用");
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function testProvider(item: SecureCredential) {
    setTestingKey(item.credentialKey);
    try {
      const result = await secureCredentialApi.testProvider({ credentialKey: item.credentialKey });
      Modal.info({
        title: result.ok ? "连接测试成功" : "连接测试失败",
        width: 720,
        content: (
          <Space direction="vertical" style={{ width: "100%" }}>
            <Text>Provider：{providerLabel(result.provider)}</Text>
            <Text>账号：{result.account || "-"}</Text>
            <Text>状态码：{result.statusCode ?? "-"}</Text>
            <Text>耗时：{result.latencyMs} ms</Text>
            <pre className="prototype-code" style={{ maxHeight: 280, overflow: "auto" }}>
              {JSON.stringify(result.detail, null, 2)}
            </pre>
          </Space>
        ),
      });
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTestingKey(null);
    }
  }

  async function rotate() {
    if (!rotating) return;
    if (!rotateSecret.trim()) {
      message.warning("请输入新密钥");
      return;
    }
    try {
      await secureCredentialApi.rotate({
        credentialKey: rotating.credentialKey,
        secret: rotateSecret.trim(),
      });
      message.success("密钥已轮换");
      setRotating(null);
      setRotateSecret("");
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function deleteItem(item: SecureCredential) {
    try {
      await secureCredentialApi.delete(item.credentialKey);
      message.success("安全凭证已删除");
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  const columns: ColumnsType<SecureCredential> = [
    {
      title: "Key",
      dataIndex: "credentialKey",
      width: 180,
      render: (value, item) => (
        <Space direction="vertical" size={0}>
          <Text strong>{value}</Text>
          <Text type="secondary">{item.displayName}</Text>
        </Space>
      ),
    },
    { title: "Provider", dataIndex: "provider", width: 110, render: providerLabel },
    { title: "关联账号", dataIndex: "accountName", width: 140 },
    { title: "类型", dataIndex: "credentialType", width: 130 },
    {
      title: "授权范围",
      dataIndex: "scopes",
      width: 180,
      render: (scopes: string[]) => scopes.map((scope) => <Tag key={scope}>{scope}</Tag>),
    },
    { title: "状态", dataIndex: "status", width: 100, render: statusTag },
    {
      title: "MCP",
      dataIndex: "allowMcp",
      width: 80,
      render: (value: boolean) => (value ? <Tag color="blue">允许</Tag> : <Tag>关闭</Tag>),
    },
    {
      title: "密钥",
      dataIndex: "hasSecret",
      width: 90,
      render: (value: boolean) => (value ? <Tag color="green">已保存</Tag> : <Tag color="orange">未保存</Tag>),
    },
    { title: "最后使用", dataIndex: "lastUsedAt", width: 150, render: (value) => value ?? "-" },
    {
      title: "操作",
      width: 260,
      fixed: "right",
      render: (_, item) => (
        <Space>
          <Button size="small" onClick={() => openEdit(item)}>
            编辑
          </Button>
          <Button
            size="small"
            loading={testingKey === item.credentialKey}
            disabled={!item.hasSecret || !item.enabled}
            onClick={() => void testProvider(item)}
          >
            测试
          </Button>
          <Button size="small" onClick={() => setRotating(item)}>
            轮换
          </Button>
          <Button size="small" onClick={() => void toggleEnabled(item, !item.enabled)}>
            {item.enabled ? "禁用" : "启用"}
          </Button>
          <Popconfirm title="确认删除该安全凭证？" onConfirm={() => void deleteItem(item)}>
            <Button size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="凭证库"
        description="保存 GitHub、GitLab、GitCode、Gitee、HTTP API 和自定义凭证，前端只展示脱敏元数据。"
        actions={
          <Space>
            <Button icon={<RefreshCw size={14} />} onClick={() => void load()} loading={loading}>
              刷新
            </Button>
            <Button type="primary" icon={<Plus size={14} />} onClick={openCreate}>
              新增凭证
            </Button>
          </Space>
        }
      />
      <OverviewCards overview={overview} />
      <Card>
        <Space style={{ marginBottom: 12 }} wrap>
          <Input.Search
            allowClear
            placeholder="搜索 Key / Provider / 账号 / 标签 / 备注"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            style={{ width: 360 }}
          />
          <Select
            allowClear
            placeholder="Provider"
            value={provider || undefined}
            options={providerOptions}
            onChange={(value) => setProvider((value ?? "") as SecureCredentialProvider | "")}
            style={{ width: 160 }}
          />
        </Space>
        <Table
          rowKey="credentialKey"
          loading={loading}
          dataSource={filteredItems}
          columns={columns}
          scroll={{ x: 1500 }}
          pagination={{ pageSize: 10, showSizeChanger: false }}
        />
      </Card>

      <Drawer
        width={560}
        title={editing ? "编辑安全凭证" : "新增安全凭证"}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        extra={
          <Button type="primary" loading={saving} onClick={() => void submit()}>
            保存
          </Button>
        }
      >
        <Form layout="vertical" form={form}>
          <Form.Item name="credentialKey" label="凭证 Key" rules={[{ required: true }]}>
            <Input disabled={Boolean(editing)} placeholder="github-main" />
          </Form.Item>
          <Form.Item name="displayName" label="显示名称" rules={[{ required: true }]}>
            <Input placeholder="GitHub 主账号" />
          </Form.Item>
          <Form.Item name="provider" label="Provider" rules={[{ required: true }]}>
            <Select
              options={providerOptions}
              onChange={(value: SecureCredentialProvider) => {
                const currentScopes = (form.getFieldValue("scopes") ?? []) as string[];
                const previousDefaultScopes = Object.values(defaultScopesByProvider).flat();
                const shouldReplaceScopes =
                  currentScopes.length === 0 || currentScopes.every((scope) => previousDefaultScopes.includes(scope));
                if (shouldReplaceScopes) {
                  form.setFieldValue("scopes", defaultScopesByProvider[value]);
                }
              }}
            />
          </Form.Item>
          <Form.Item name="credentialType" label="凭证类型" rules={[{ required: true }]}>
            <Select options={credentialTypeOptions} />
          </Form.Item>
          <Form.Item name="secret" label={editing ? "Secret（留空表示保留原密钥）" : "Secret"}>
            <Input.Password placeholder="只写入后端密文，不会回显" />
          </Form.Item>
          <Form.Item name="accountName" label="关联账号">
            <Input placeholder="用户名、组织或服务账号" />
          </Form.Item>
          <Form.Item name="baseUrl" label="API Base URL">
            <Input placeholder="GitLab / GitCode / Gitee / HTTP API 可填写" />
          </Form.Item>
          <Form.Item name="scopes" label="授权范围">
            <Select
              mode="tags"
              options={scopeOptionsByProvider[selectedProvider ?? "github"]}
              optionFilterProp="label"
              placeholder="选择读写权限，也可输入自定义 scope"
            />
          </Form.Item>
          <Form.Item name="tags" label="标签">
            <Select
              mode="tags"
              options={tagOptions}
              optionFilterProp="label"
              placeholder="选择常用标签，或输入新标签后回车"
            />
          </Form.Item>
          <Form.Item name="folder" label="文件夹">
            <Input placeholder="默认" />
          </Form.Item>
          <Form.Item name="approvalPolicy" label="审批策略">
            <Select options={approvalPolicyOptions} />
          </Form.Item>
          <Form.Item name="expiresAt" label="过期时间">
            <Input placeholder="例如 2026-12-31 23:59:59，留空表示不设置" />
          </Form.Item>
          <Form.Item name="allowMcp" label="允许 MCP 使用" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="description" label="备注">
            <Input.TextArea rows={3} placeholder="不要在备注中填写密钥明文" />
          </Form.Item>
        </Form>
      </Drawer>

      <Modal
        title="轮换密钥"
        open={Boolean(rotating)}
        onOk={() => void rotate()}
        onCancel={() => {
          setRotating(null);
          setRotateSecret("");
        }}
      >
        <Space direction="vertical" style={{ width: "100%" }}>
          <Text type="secondary">当前凭证：{rotating?.credentialKey}</Text>
          <Input.Password
            value={rotateSecret}
            onChange={(event) => setRotateSecret(event.target.value)}
            placeholder="输入新密钥，旧密钥不会回显"
          />
        </Space>
      </Modal>
    </div>
  );
}

export function SecureCredentialSessionsPage() {
  const { items, loading: credentialLoading, load: loadCredentials } = useSecureCredentialData();
  const { sessions, loading: sessionLoading, load: loadSessions } = useSecureCredentialSessions();
  const [form] = Form.useForm();
  const selectedCredentialKey = Form.useWatch("credentialKey", form) as string | undefined;
  const [modalOpen, setModalOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [statusFilter, setStatusFilter] = useState("");

  const usableCredentials = useMemo(
    () => items.filter((item) => item.enabled && item.allowMcp && item.status === "active" && item.hasSecret),
    [items],
  );

  const filteredSessions = useMemo(
    () => sessions.filter((item) => !statusFilter || item.status === statusFilter),
    [sessions, statusFilter],
  );

  const selectedSessionCredential = useMemo(
    () => usableCredentials.find((item) => item.credentialKey === selectedCredentialKey),
    [selectedCredentialKey, usableCredentials],
  );

  const sessionScopeOptions = useMemo(
    () =>
      (selectedSessionCredential?.scopes ?? []).map((scope) => ({
        label: scope,
        value: scope,
      })),
    [selectedSessionCredential],
  );

  async function refresh() {
    await Promise.all([loadCredentials(), loadSessions()]);
  }

  function openCreateSession() {
    form.setFieldsValue({
      credentialKey: usableCredentials[0]?.credentialKey,
      caller: "local-user",
      scopes: usableCredentials[0]?.scopes ?? [],
      ttlMinutes: 30,
    });
    setModalOpen(true);
  }

  async function createSession() {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const session = await secureCredentialApi.createSession({
        credentialKey: values.credentialKey,
        caller: values.caller,
        scopes: values.scopes ?? [],
        ttlMinutes: values.ttlMinutes,
      });
      message.success(`会话已创建：${session.sessionId}`);
      setModalOpen(false);
      await loadSessions();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  async function checkStatus(session: SecureCredentialSession) {
    try {
      const result = await secureCredentialApi.sessionStatus(session.sessionId);
      message.info(result.valid ? "会话有效" : result.reason);
      await loadSessions();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function revoke(session: SecureCredentialSession) {
    try {
      await secureCredentialApi.revokeSession(session.sessionId);
      message.success("会话已吊销");
      await loadSessions();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  const columns: ColumnsType<SecureCredentialSession> = [
    {
      title: "Session ID",
      dataIndex: "sessionId",
      width: 220,
      render: (value: string) => <Text code>{value}</Text>,
    },
    { title: "Provider", dataIndex: "provider", width: 110, render: providerLabel },
    { title: "凭证 Key", dataIndex: "credentialKey", width: 180 },
    { title: "调用方", dataIndex: "caller", width: 130 },
    {
      title: "Scope",
      dataIndex: "scopes",
      width: 180,
      render: (scopes: string[]) => scopes.map((scope) => <Tag key={scope}>{scope}</Tag>),
    },
    { title: "状态", dataIndex: "status", width: 100, render: sessionStatusTag },
    { title: "过期时间", dataIndex: "expiresAt", width: 170 },
    { title: "调用次数", dataIndex: "callCount", width: 90 },
    {
      title: "操作",
      width: 170,
      fixed: "right",
      render: (_, item) => (
        <Space>
          <Button size="small" onClick={() => void checkStatus(item)}>
            校验
          </Button>
          <Popconfirm title="确认吊销该会话？" onConfirm={() => void revoke(item)}>
            <Button size="small" disabled={item.status === "revoked"} danger>
              吊销
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="会话"
        description="管理 AI/MCP 使用安全凭证时创建的短期受控会话。"
        actions={
          <Space>
            <Button icon={<RefreshCw size={14} />} loading={sessionLoading} onClick={() => void refresh()}>
              刷新
            </Button>
            <Button type="primary" icon={<Plus size={14} />} onClick={openCreateSession}>
              创建会话
            </Button>
          </Space>
        }
      />
      <Card>
        <Space style={{ marginBottom: 12 }} wrap>
          <Select
            allowClear
            placeholder="会话状态"
            style={{ width: 160 }}
            value={statusFilter || undefined}
            options={[
              { label: "有效", value: "active" },
              { label: "已过期", value: "expired" },
              { label: "已吊销", value: "revoked" },
            ]}
            onChange={(value) => setStatusFilter(value ?? "")}
          />
          <Text type="secondary">
            只显示 session 句柄，不暴露 GitHub / GitLab / GitCode / Gitee / HTTP API 的真实密钥。
          </Text>
        </Space>
        <Table
          rowKey="sessionId"
          loading={sessionLoading}
          dataSource={filteredSessions}
          columns={columns}
          scroll={{ x: 1250 }}
          pagination={{ pageSize: 10, showSizeChanger: false }}
        />
      </Card>

      <Modal
        title="创建安全会话"
        open={modalOpen}
        confirmLoading={saving}
        onOk={() => void createSession()}
        onCancel={() => setModalOpen(false)}
      >
        <Form layout="vertical" form={form}>
          <Form.Item name="credentialKey" label="凭证" rules={[{ required: true }]}>
            <Select
              loading={credentialLoading}
              options={usableCredentials.map((item) => ({
                label: `${item.displayName} (${item.credentialKey})`,
                value: item.credentialKey,
              }))}
              placeholder="请选择已启用且允许 MCP 使用的凭证"
              onChange={(value) => {
                const credential = usableCredentials.find((item) => item.credentialKey === value);
                form.setFieldValue("scopes", credential?.scopes ?? []);
              }}
            />
          </Form.Item>
          <Form.Item name="caller" label="调用方">
            <Input placeholder="local-user / codex / claude-code" />
          </Form.Item>
          <Form.Item name="scopes" label="会话范围">
            <Select
              mode="multiple"
              options={sessionScopeOptions}
              optionFilterProp="label"
              placeholder="默认使用凭证授权范围"
            />
          </Form.Item>
          <Form.Item name="ttlMinutes" label="有效期（分钟）">
            <InputNumber min={1} max={240} style={{ width: "100%" }} />
          </Form.Item>
        </Form>
        {usableCredentials.length === 0 ? (
          <Paragraph type="secondary">
            当前没有可创建会话的凭证。请先在凭证库保存密钥、启用凭证，并打开“允许 MCP 使用”。
          </Paragraph>
        ) : null}
      </Modal>
    </div>
  );
}

export function SecureCredentialMcpPage() {
  const [tools, setTools] = useState<McpToolPermission[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const overview = await mcpApi.overview();
      setTools(overview.tools.filter((tool) => isSecureCredentialMcpTool(tool.tool)));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="prototype-page">
      <PageHeader
        title="MCP 接入"
        description="管理安全凭证相关 MCP 工具和客户端接入状态。"
        actions={
          <Button icon={<RefreshCw size={14} />} loading={loading} onClick={() => void load()}>
            刷新
          </Button>
        }
      />
      <Card title="安全凭证 MCP 工具">
        <Table
          rowKey="tool"
          loading={loading}
          dataSource={tools}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          columns={[
            { title: "工具", dataIndex: "tool", width: 240, render: (value) => <Text code>{value}</Text> },
            { title: "策略", dataIndex: "policy", width: 360 },
            { title: "审计", dataIndex: "audit" },
          ]}
        />
      </Card>
    </div>
  );
}

export function SecureCredentialAuditPage() {
  const [items, setItems] = useState<SecureCredentialAuditLog[]>([]);
  const [loading, setLoading] = useState(false);
  const [keyword, setKeyword] = useState("");
  const [provider, setProvider] = useState<SecureCredentialProvider | "">("");
  const [result, setResult] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setItems(
        await secureCredentialApi.listAuditLogs({
          keyword: keyword.trim() || undefined,
          provider: provider || undefined,
          result: result || undefined,
          limit: 500,
        }),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [keyword, provider, result]);

  useEffect(() => {
    void load();
  }, [load]);

  const total = items.length;
  const success = items.filter((item) => item.result === "success").length;
  const failure = items.filter((item) => item.result !== "success").length;
  const rejected = items.filter((item) => item.result === "rejected").length;

  return (
    <div className="prototype-page">
      <PageHeader
        title="安全凭证审计"
        description="查看凭证变更、会话签发和 MCP 调用审计。"
        actions={
          <Button icon={<RefreshCw size={14} />} loading={loading} onClick={() => void load()}>
            刷新
          </Button>
        }
      />
      <div className="prototype-grid prototype-grid-4">
        <Card>
          <Statistic title="总调用" value={total} />
        </Card>
        <Card>
          <Statistic title="成功率" value={total ? (success / total) * 100 : 0} precision={1} suffix="%" />
        </Card>
        <Card>
          <Statistic title="失败次数" value={failure} />
        </Card>
        <Card>
          <Statistic title="被拒次数" value={rejected} />
        </Card>
      </div>
      <Card>
        <Space style={{ marginBottom: 12 }} wrap>
          <Input.Search
            allowClear
            placeholder="搜索 actor / action / detail"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            onSearch={() => void load()}
            style={{ width: 320 }}
          />
          <Select
            allowClear
            placeholder="Provider"
            value={provider || undefined}
            options={providerOptions}
            onChange={(value) => setProvider((value ?? "") as SecureCredentialProvider | "")}
            style={{ width: 160 }}
          />
          <Select
            allowClear
            placeholder="结果"
            value={result || undefined}
            options={[
              { label: "成功", value: "success" },
              { label: "失败", value: "failure" },
              { label: "拒绝", value: "rejected" },
            ]}
            onChange={(value) => setResult(value ?? "")}
            style={{ width: 140 }}
          />
        </Space>
        <Table
          rowKey="id"
          loading={loading}
          dataSource={items}
          scroll={{ x: 1500 }}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          columns={[
            { title: "时间", dataIndex: "createdAt", width: 170 },
            { title: "调用方", dataIndex: "actor", width: 130 },
            { title: "来源", dataIndex: "source", width: 150 },
            { title: "Provider", dataIndex: "provider", width: 110, render: providerLabel },
            { title: "凭证 Key", dataIndex: "credentialKey", width: 180 },
            { title: "动作", dataIndex: "action", width: 200 },
            { title: "风险", dataIndex: "risk", width: 100, render: (value) => <Tag>{value}</Tag> },
            {
              title: "结果",
              dataIndex: "result",
              width: 100,
              render: (value) => <Tag color={value === "success" ? "green" : "red"}>{value}</Tag>,
            },
            { title: "耗时", dataIndex: "durationMs", width: 90, render: (value) => `${value} ms` },
            {
              title: "详情",
              dataIndex: "detailJson",
              render: (value) => (
                <pre className="prototype-code" style={{ maxHeight: 120, overflow: "auto", margin: 0 }}>
                  {value}
                </pre>
              ),
            },
          ]}
        />
      </Card>
    </div>
  );
}

export function SecureCredentialPoliciesPage() {
  const [settings, setSettings] = useState<SecureCredentialPolicySettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setSettings(await secureCredentialApi.policySettings());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function save(next: SecureCredentialPolicySettings) {
    setSettings(next);
    setSaving(true);
    try {
      const saved = await secureCredentialApi.updatePolicySettings({
        defaultSessionTtlMinutes: next.defaultSessionTtlMinutes,
        maxResponseItems: next.maxResponseItems,
        allowReadonlyAuto: next.allowReadonlyAuto,
        requireApprovalForAll: next.requireApprovalForAll,
        allowHttpCustomHeaders: next.allowHttpCustomHeaders,
        httpAllowedDomains: next.httpAllowedDomains,
        rateLimitPerMinute: next.rateLimitPerMinute,
        maxConcurrentSessions: next.maxConcurrentSessions,
        allowDefaultBranchCommits: next.allowDefaultBranchCommits,
        allowHighRiskRepoOps: next.allowHighRiskRepoOps,
        allowDeleteBranch: next.allowDeleteBranch,
        allowDeleteTag: next.allowDeleteTag,
        allowDeleteRelease: next.allowDeleteRelease,
        allowUpdateRef: next.allowUpdateRef,
        allowUpdateRepoSettings: next.allowUpdateRepoSettings,
      });
      setSettings(saved);
      message.success("策略已保存");
    } catch (error) {
      message.error(getErrorMessage(error));
      void load();
    } finally {
      setSaving(false);
    }
  }

  async function update<K extends keyof SecureCredentialPolicySettings>(
    field: K,
    value: SecureCredentialPolicySettings[K],
  ) {
    if (!settings) return;
    const next = { ...settings, [field]: value };
    if (field === "allowHighRiskRepoOps" && !value) {
      next.allowDeleteBranch = false;
      next.allowDeleteTag = false;
      next.allowDeleteRelease = false;
      next.allowUpdateRef = false;
      next.allowUpdateRepoSettings = false;
    }
    await save(next);
  }

  const current = settings ?? createDefaultPolicySettings();

  return (
    <div className="prototype-page">
      <PageHeader title="策略" description="配置 AI/MCP 使用凭证的默认会话、审批、脱敏和限流策略。" />
      <Card loading={loading} title="基础策略" style={{ marginBottom: 16 }}>
        <Space direction="vertical" size={16} className="w-full">
          <div className="prototype-grid prototype-grid-2">
            <Card size="small">
              <Text strong>默认会话 TTL（分钟）</Text>
              <InputNumber
                min={1}
                max={240}
                value={current.defaultSessionTtlMinutes}
                disabled={saving}
                style={{ width: "100%", marginTop: 8 }}
                onChange={(value) => void update("defaultSessionTtlMinutes", Number(value ?? 30))}
              />
            </Card>
            <Card size="small">
              <Text strong>最大返回条数</Text>
              <InputNumber
                min={1}
                max={500}
                value={current.maxResponseItems}
                disabled={saving}
                style={{ width: "100%", marginTop: 8 }}
                onChange={(value) => void update("maxResponseItems", Number(value ?? 100))}
              />
            </Card>
            <Card size="small">
              <Text strong>单分钟调用限制</Text>
              <InputNumber
                min={1}
                max={600}
                value={current.rateLimitPerMinute}
                disabled={saving}
                style={{ width: "100%", marginTop: 8 }}
                onChange={(value) => void update("rateLimitPerMinute", Number(value ?? 60))}
              />
            </Card>
            <Card size="small">
              <Text strong>单凭证并发会话限制</Text>
              <InputNumber
                min={1}
                max={100}
                value={current.maxConcurrentSessions}
                disabled={saving}
                style={{ width: "100%", marginTop: 8 }}
                onChange={(value) => void update("maxConcurrentSessions", Number(value ?? 5))}
              />
            </Card>
          </div>
          <div className="prototype-grid prototype-grid-2">
            {[
              ["allowReadonlyAuto", "允许只读自动执行", "关闭后，Git/HTTP 只读 Provider 调用也会被拒绝并提示需要审批。"],
              ["requireApprovalForAll", "全部操作需审批", "开启后，所有 Provider 调用默认拒绝自动执行，等待后续审批入口接入。"],
              ["allowHttpCustomHeaders", "允许 HTTP 自定义 Header", "首版仅保存策略，不向 MCP 暴露自定义 Header 明文。"],
              ["allowDefaultBranchCommits", "允许默认分支直接提交", "默认关闭。开启后仍必须经过审批和 requestHash 校验。"],
            ].map(([field, title, description]) => (
              <Card key={field} size="small">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <Text strong>{title}</Text>
                    <Paragraph type="secondary" className="!mb-0">
                      {description}
                    </Paragraph>
                  </div>
                  <Switch
                    checked={Boolean(current[field as keyof SecureCredentialPolicySettings])}
                    loading={saving}
                    onChange={(checked) =>
                      void update(field as keyof SecureCredentialPolicySettings, checked as never)
                    }
                  />
                </div>
              </Card>
            ))}
          </div>
          <Card size="small">
            <Text strong>HTTP API 允许域名白名单</Text>
            <Select
              mode="tags"
              value={current.httpAllowedDomains}
              disabled={saving}
              placeholder="例如 api.github.com / gitlab.com / *.example.com；留空表示不限制"
              style={{ width: "100%", marginTop: 8 }}
              onChange={(value) => void update("httpAllowedDomains", value)}
            />
          </Card>
        </Space>
      </Card>
      <Card loading={loading} title="高风险仓库操作策略">
        <Space direction="vertical" size={16} className="w-full">
          <div className="flex items-center justify-between gap-4">
            <div>
              <Text strong>允许审批后执行高风险仓库操作</Text>
              <Paragraph type="secondary" className="!mb-0">
                未开启时，即使审批通过，删除分支、删除 tag、删除 release、更新 Git ref 和修改仓库设置也会被拒绝。
              </Paragraph>
            </div>
            <Switch
              checked={current.allowHighRiskRepoOps}
              loading={saving}
              onChange={(checked) => void update("allowHighRiskRepoOps", checked)}
            />
          </div>
          <div className="prototype-grid prototype-grid-2">
            {[
              ["allowDeleteBranch", "删除分支", "允许 approved 后删除 GitHub/GitLab/GitCode/Gitee 分支。"],
              ["allowDeleteTag", "删除 tag", "允许 approved 后删除 GitHub/GitLab/GitCode/Gitee tag。"],
              ["allowDeleteRelease", "删除 release", "允许 approved 后删除 GitHub/GitLab/GitCode/Gitee release。"],
              ["allowUpdateRef", "更新 Git ref", "允许 approved 后更新 Git 引用，默认禁止强制更新。"],
              [
                "allowUpdateRepoSettings",
                "修改仓库设置",
                "允许 approved 后修改仓库或项目基础设置。",
              ],
            ].map(([field, title, description]) => (
              <Card key={field} size="small">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <Text strong>{title}</Text>
                    <Paragraph type="secondary" className="!mb-0">
                      {description}
                    </Paragraph>
                  </div>
                  <Switch
                    checked={Boolean(current[field as keyof SecureCredentialPolicySettings])}
                    disabled={!current.allowHighRiskRepoOps}
                    loading={saving}
                    onChange={(checked) =>
                      void update(field as keyof SecureCredentialPolicySettings, checked as never)
                    }
                  />
                </div>
              </Card>
            ))}
          </div>
          <Text type="secondary">
            当前策略更新时间：{current.updatedAt ?? "尚未保存，使用默认拒绝策略"}
          </Text>
        </Space>
      </Card>
    </div>
  );
}

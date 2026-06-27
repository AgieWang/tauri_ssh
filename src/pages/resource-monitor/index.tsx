import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Progress,
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
import { Activity, Database, HardDrive, MemoryStick, Network, RefreshCw, Server } from "lucide-react";
import { getErrorMessage, resourceMonitorApi } from "@/lib/api";
import type {
  CollectResourceBatchResult,
  ResourceAlertEvent,
  ResourceAlertRule,
  ResourceMetricSnapshot,
  ResourceMonitorOverview,
  ResourceMonitorTarget,
  UpsertResourceAlertRuleInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

const targetTypeMeta: Record<string, { label: string; color: string }> = {
  server: { label: "服务器", color: "blue" },
  mysql: { label: "MySQL", color: "green" },
  postgresql: { label: "PostgreSQL", color: "cyan" },
  redis: { label: "Redis", color: "red" },
};

const statusMeta: Record<string, { label: string; color: string; percentStatus?: "success" | "exception" | "normal" }> = {
  unknown: { label: "未采集", color: "default" },
  healthy: { label: "正常", color: "green", percentStatus: "success" },
  warning: { label: "预警", color: "orange", percentStatus: "exception" },
  failed: { label: "失败", color: "red", percentStatus: "exception" },
};

const severityMeta: Record<string, { label: string; color: string }> = {
  info: { label: "提示", color: "blue" },
  warning: { label: "警告", color: "orange" },
  critical: { label: "严重", color: "red" },
};

const metricOptions = [
  { label: "CPU 使用率", value: "cpuUsagePercent", targetTypes: ["server"] },
  { label: "内存使用率", value: "memoryUsagePercent", targetTypes: ["server", "redis"] },
  { label: "磁盘使用率", value: "diskUsagePercent", targetTypes: ["server"] },
  { label: "数据库连接使用率", value: "connectionUsagePercent", targetTypes: ["mysql", "postgresql"] },
  { label: "活动连接数", value: "activeConnections", targetTypes: ["mysql", "postgresql"] },
  { label: "缓存命中率", value: "cacheHitPercent", targetTypes: ["mysql", "postgresql"] },
  { label: "锁等待", value: "lockWaits", targetTypes: ["mysql", "postgresql"] },
  { label: "慢查询", value: "slowQueries", targetTypes: ["mysql"] },
  { label: "Redis 客户端连接", value: "connectedClients", targetTypes: ["redis"] },
  { label: "Redis Key 数", value: "keyCount", targetTypes: ["redis"] },
  { label: "Redis 命中率", value: "hitPercent", targetTypes: ["redis"] },
  { label: "Redis 慢日志", value: "slowlogLen", targetTypes: ["redis"] },
];

function numberValue(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  return null;
}

function summaryNumber(snapshot: ResourceMetricSnapshot | null | undefined, key: string) {
  return numberValue(snapshot?.summary?.[key]);
}

function formatPercent(value: number | null) {
  return value === null ? "-" : `${value.toFixed(1)}%`;
}

function formatBytes(value: number | null) {
  if (value === null) return "-";
  if (value >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} GB/s`;
  if (value >= 1024 ** 2) return `${(value / 1024 ** 2).toFixed(1)} MB/s`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB/s`;
  return `${value.toFixed(0)} B/s`;
}

function formatStaticBytes(value: number | null) {
  if (value === null) return "-";
  if (value >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} GB`;
  if (value >= 1024 ** 2) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${value.toFixed(0)} B`;
}

function formatRedisMaxMemory(value: number | null) {
  if (value === 0) return "未限制";
  return formatStaticBytes(value);
}

function formatCount(value: number | null, suffix = "") {
  return value === null ? "-" : `${value.toFixed(0)}${suffix}`;
}

function MetricProgress({ value }: { value: number | null }) {
  if (value === null) return <Text type="secondary">-</Text>;
  return (
    <Progress
      percent={Number(value.toFixed(1))}
      size="small"
      status={value >= 90 ? "exception" : value >= 75 ? "normal" : "success"}
    />
  );
}

function renderDetailMetricCards(targetType: string, snapshot: ResourceMetricSnapshot | null | undefined) {
  if (targetType === "server") {
    return (
      <div className="prototype-grid prototype-grid-3">
        <Card>
          <Statistic title="CPU 使用率" value={formatPercent(summaryNumber(snapshot, "cpuUsagePercent"))} />
        </Card>
        <Card>
          <Statistic title="内存使用率" value={formatPercent(summaryNumber(snapshot, "memoryUsagePercent"))} />
        </Card>
        <Card>
          <Statistic title="磁盘使用率" value={formatPercent(summaryNumber(snapshot, "diskUsagePercent"))} />
        </Card>
      </div>
    );
  }

  if (targetType === "redis") {
    const memoryUsage = summaryNumber(snapshot, "memoryUsagePercent");
    return (
      <div className="prototype-grid prototype-grid-4">
        <Card>
          <Statistic title="客户端连接" value={formatCount(summaryNumber(snapshot, "connectedClients"))} />
        </Card>
        <Card>
          <Statistic
            title="内存"
            value={
              memoryUsage === null
                ? formatStaticBytes(summaryNumber(snapshot, "usedMemoryBytes"))
                : formatPercent(memoryUsage)
            }
          />
        </Card>
        <Card>
          <Statistic title="Key 数" value={formatCount(summaryNumber(snapshot, "keyCount"))} />
        </Card>
        <Card>
          <Statistic title="命中率" value={formatPercent(summaryNumber(snapshot, "hitPercent"))} />
        </Card>
      </div>
    );
  }

  return (
    <div className="prototype-grid prototype-grid-4">
      <Card>
        <Statistic title="活动连接" value={formatCount(summaryNumber(snapshot, "activeConnections"))} />
      </Card>
      <Card>
        <Statistic title="连接使用率" value={formatPercent(summaryNumber(snapshot, "connectionUsagePercent"))} />
      </Card>
      <Card>
        <Statistic title="缓存命中率" value={formatPercent(summaryNumber(snapshot, "cacheHitPercent"))} />
      </Card>
      <Card>
        <Statistic title="库容量" value={formatStaticBytes(summaryNumber(snapshot, "databaseSizeBytes"))} />
      </Card>
    </div>
  );
}

function renderDetailStatusCard(targetType: string, snapshot: ResourceMetricSnapshot | null | undefined) {
  if (targetType === "server") {
    return (
      <Card title="吞吐">
        <Space size={32} wrap>
          <Space>
            <Network size={16} />
            <Text>网络 RX：{formatBytes(summaryNumber(snapshot, "networkRxBytesPerSec"))}</Text>
          </Space>
          <Text>网络 TX：{formatBytes(summaryNumber(snapshot, "networkTxBytesPerSec"))}</Text>
          <Text>磁盘读：{formatBytes(summaryNumber(snapshot, "diskReadBytesPerSec"))}</Text>
          <Text>磁盘写：{formatBytes(summaryNumber(snapshot, "diskWriteBytesPerSec"))}</Text>
        </Space>
      </Card>
    );
  }

  if (targetType === "redis") {
    return (
      <Card title="Redis 状态">
        <Space size={32} wrap>
          <Text>已用内存：{formatStaticBytes(summaryNumber(snapshot, "usedMemoryBytes"))}</Text>
          <Text>最大内存：{formatRedisMaxMemory(summaryNumber(snapshot, "maxMemoryBytes"))}</Text>
          <Text>过期 Key：{formatCount(summaryNumber(snapshot, "expiredKeys"))}</Text>
          <Text>淘汰 Key：{formatCount(summaryNumber(snapshot, "evictedKeys"))}</Text>
          <Text>慢日志：{formatCount(summaryNumber(snapshot, "slowlogLen"))}</Text>
        </Space>
      </Card>
    );
  }

  return (
    <Card title={targetType === "postgresql" ? "PostgreSQL 状态" : "MySQL 状态"}>
      <Space size={32} wrap>
        <Text>最大连接：{formatCount(summaryNumber(snapshot, "maxConnections"))}</Text>
        <Text>表数量：{formatCount(summaryNumber(snapshot, "tableCount"))}</Text>
        <Text>慢查询：{formatCount(summaryNumber(snapshot, "slowQueries"))}</Text>
        <Text>锁等待：{formatCount(summaryNumber(snapshot, "lockWaits"))}</Text>
      </Space>
    </Card>
  );
}

function buildHistoryColumns(targetType: string): ColumnsType<ResourceMetricSnapshot> {
  const baseColumns: ColumnsType<ResourceMetricSnapshot> = [
    { title: "时间", dataIndex: "collectedAt", width: 170 },
    {
      title: "状态",
      dataIndex: "status",
      width: 90,
      render: (value) => (
        <Tag color={statusMeta[String(value)]?.color ?? "default"}>
          {statusMeta[String(value)]?.label ?? String(value)}
        </Tag>
      ),
    },
  ];

  if (targetType === "server") {
    return [
      ...baseColumns,
      { title: "CPU", render: (_, row) => formatPercent(summaryNumber(row, "cpuUsagePercent")) },
      { title: "内存", render: (_, row) => formatPercent(summaryNumber(row, "memoryUsagePercent")) },
      { title: "磁盘", render: (_, row) => formatPercent(summaryNumber(row, "diskUsagePercent")) },
      { title: "耗时", dataIndex: "durationMs", render: (value) => `${value} ms` },
    ];
  }

  if (targetType === "redis") {
    return [
      ...baseColumns,
      { title: "连接", render: (_, row) => formatCount(summaryNumber(row, "connectedClients")) },
      {
        title: "内存",
        render: (_, row) => {
          const memoryUsage = summaryNumber(row, "memoryUsagePercent");
          return memoryUsage === null
            ? formatStaticBytes(summaryNumber(row, "usedMemoryBytes"))
            : formatPercent(memoryUsage);
        },
      },
      { title: "Key 数", render: (_, row) => formatCount(summaryNumber(row, "keyCount")) },
      { title: "命中率", render: (_, row) => formatPercent(summaryNumber(row, "hitPercent")) },
      { title: "耗时", dataIndex: "durationMs", render: (value) => `${value} ms` },
    ];
  }

  return [
    ...baseColumns,
    { title: "活动连接", render: (_, row) => formatCount(summaryNumber(row, "activeConnections")) },
    { title: "连接使用率", render: (_, row) => formatPercent(summaryNumber(row, "connectionUsagePercent")) },
    { title: "缓存命中率", render: (_, row) => formatPercent(summaryNumber(row, "cacheHitPercent")) },
    { title: "库容量", render: (_, row) => formatStaticBytes(summaryNumber(row, "databaseSizeBytes")) },
    { title: "耗时", dataIndex: "durationMs", render: (value) => `${value} ms` },
  ];
}

function trendMetricConfigs(targetType: string) {
  if (targetType === "server") {
    return [
      { key: "cpuUsagePercent", label: "CPU 使用率", formatter: formatPercent },
      { key: "memoryUsagePercent", label: "内存使用率", formatter: formatPercent },
      { key: "diskUsagePercent", label: "磁盘使用率", formatter: formatPercent },
    ];
  }
  if (targetType === "redis") {
    return [
      { key: "connectedClients", label: "客户端连接", formatter: (value: number | null) => formatCount(value) },
      { key: "usedMemoryBytes", label: "已用内存", formatter: formatStaticBytes },
      { key: "keyCount", label: "Key 数", formatter: (value: number | null) => formatCount(value) },
      { key: "hitPercent", label: "命中率", formatter: formatPercent },
    ];
  }
  return [
    { key: "activeConnections", label: "活动连接", formatter: (value: number | null) => formatCount(value) },
    { key: "connectionUsagePercent", label: "连接使用率", formatter: formatPercent },
    { key: "cacheHitPercent", label: "缓存命中率", formatter: formatPercent },
    { key: "databaseSizeBytes", label: "库容量", formatter: formatStaticBytes },
  ];
}

function TrendCard({
  title,
  snapshots,
  metricKey,
  formatter,
}: {
  title: string;
  snapshots: ResourceMetricSnapshot[];
  metricKey: string;
  formatter: (value: number | null) => string;
}) {
  const points = snapshots
    .slice()
    .reverse()
    .map((item) => summaryNumber(item, metricKey))
    .filter((value): value is number => value !== null);
  const latest = points.length ? points[points.length - 1] : null;
  const min = points.length ? Math.min(...points) : 0;
  const max = points.length ? Math.max(...points) : 0;
  const range = max - min || 1;
  const width = 220;
  const height = 64;
  const polyline = points
    .map((value, index) => {
      const x = points.length <= 1 ? width : (index / (points.length - 1)) * width;
      const y = height - ((value - min) / range) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <Card size="small">
      <Space direction="vertical" size={8} style={{ width: "100%" }}>
        <Space style={{ width: "100%", justifyContent: "space-between" }}>
          <Text type="secondary">{title}</Text>
          <Text strong>{formatter(latest)}</Text>
        </Space>
        {points.length > 1 ? (
          <svg viewBox={`0 0 ${width} ${height}`} width="100%" height={height} role="img" aria-label={`${title}趋势`}>
            <polyline
              points={polyline}
              fill="none"
              stroke="var(--color-primary, #1677ff)"
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
            />
          </svg>
        ) : (
          <Text type="secondary">暂无足够趋势数据</Text>
        )}
      </Space>
    </Card>
  );
}

export default function ResourceMonitorPage() {
  const [ruleForm] = Form.useForm<UpsertResourceAlertRuleInput>();
  const [targets, setTargets] = useState<ResourceMonitorTarget[]>([]);
  const [overview, setOverview] = useState<ResourceMonitorOverview | null>(null);
  const [history, setHistory] = useState<ResourceMetricSnapshot[]>([]);
  const [alertRules, setAlertRules] = useState<ResourceAlertRule[]>([]);
  const [alertEvents, setAlertEvents] = useState<ResourceAlertEvent[]>([]);
  const [detailAlerts, setDetailAlerts] = useState<ResourceAlertEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [collectingKey, setCollectingKey] = useState<string | null>(null);
  const [batchResult, setBatchResult] = useState<CollectResourceBatchResult | null>(null);
  const [selected, setSelected] = useState<ResourceMonitorTarget | null>(null);
  const [ruleModalOpen, setRuleModalOpen] = useState(false);
  const [editingRule, setEditingRule] = useState<ResourceAlertRule | null>(null);
  const [typeFilter, setTypeFilter] = useState<string>("all");
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [groupFilter, setGroupFilter] = useState<string>("all");
  const [keyword, setKeyword] = useState("");

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [targetRows, overviewValue, rules, events] = await Promise.all([
        resourceMonitorApi.listTargets(),
        resourceMonitorApi.overview(),
        resourceMonitorApi.listAlertRules({}),
        resourceMonitorApi.listAlertEvents({ status: "open", limit: 100 }),
      ]);
      setTargets(targetRows);
      setOverview(overviewValue);
      setAlertRules(rules);
      setAlertEvents(events);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const collectTarget = async (target: ResourceMonitorTarget) => {
    const key = `${target.targetType}:${target.targetKey}`;
    setCollectingKey(key);
    try {
      if (target.targetType === "server") {
        await resourceMonitorApi.collectServer(target.targetKey);
      } else if (target.targetType === "redis") {
        await resourceMonitorApi.collectRedis(target.targetKey);
      } else {
        await resourceMonitorApi.collectDatabase(target.targetKey);
      }
      message.success("资源采集完成");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setCollectingKey(null);
    }
  };

  const collectBatch = async () => {
    setCollectingKey("__batch__");
    try {
      const result = await resourceMonitorApi.collectBatch({ onlyEnabled: true });
      setBatchResult(result);
      message.success(`批量采集完成：成功 ${result.success}，失败 ${result.failed}`);
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setCollectingKey(null);
    }
  };

  const openDetail = async (target: ResourceMonitorTarget) => {
    setSelected(target);
    try {
      const [rows, events] = await Promise.all([
        resourceMonitorApi.listSnapshots({
          targetType: target.targetType,
          targetKey: target.targetKey,
          limit: 50,
        }),
        resourceMonitorApi.listAlertEvents({
          targetType: target.targetType,
          targetKey: target.targetKey,
          limit: 20,
        }),
      ]);
      setHistory(rows);
      setDetailAlerts(events);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const openRuleModal = (rule?: ResourceAlertRule) => {
    setEditingRule(rule ?? null);
    ruleForm.setFieldsValue({
      id: rule?.id ?? null,
      targetType: rule?.targetType ?? "server",
      targetKey: rule?.targetKey ?? "*",
      metricKey: rule?.metricKey ?? "cpuUsagePercent",
      operator: rule?.operator ?? ">",
      thresholdValue: rule?.thresholdValue ?? 90,
      severity: rule?.severity ?? "warning",
      enabled: rule?.enabled ?? true,
    });
    setRuleModalOpen(true);
  };

  const submitRule = async () => {
    try {
      const values = await ruleForm.validateFields();
      await resourceMonitorApi.upsertAlertRule({
        ...values,
        id: editingRule?.id ?? values.id ?? null,
        targetKey: values.targetKey?.trim() || "*",
      });
      message.success(editingRule ? "告警规则已更新" : "告警规则已创建");
      setRuleModalOpen(false);
      setEditingRule(null);
      await loadData();
    } catch (error) {
      if (error && typeof error === "object" && "errorFields" in error) return;
      message.error(getErrorMessage(error));
    }
  };

  const toggleRule = async (rule: ResourceAlertRule, enabled: boolean) => {
    try {
      await resourceMonitorApi.upsertAlertRule({
        id: rule.id,
        targetType: rule.targetType,
        targetKey: rule.targetKey,
        metricKey: rule.metricKey,
        operator: rule.operator,
        thresholdValue: rule.thresholdValue,
        severity: rule.severity,
        enabled,
      });
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const deleteRule = async (id: number) => {
    try {
      await resourceMonitorApi.deleteAlertRule(id);
      message.success("告警规则已删除");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const resolveAlert = async (id: number) => {
    try {
      await resourceMonitorApi.resolveAlertEvent(id);
      message.success("告警事件已解决");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const columns = useMemo<ColumnsType<ResourceMonitorTarget>>(
    () => [
      {
        title: "资源",
        dataIndex: "displayName",
        width: 180,
        render: (_, row) => (
          <Space direction="vertical" size={0}>
            <Space size={6}>
              {row.targetType === "server" ? <Server size={15} /> : <Database size={15} />}
              <Text strong>{row.displayName}</Text>
            </Space>
            <Text type="secondary" className="text-xs">
              {row.targetKey}
            </Text>
          </Space>
        ),
      },
      {
        title: "类型",
        dataIndex: "targetType",
        width: 110,
        render: (value: string) => (
          <Tag color={targetTypeMeta[value]?.color ?? "default"}>
            {targetTypeMeta[value]?.label ?? value}
          </Tag>
        ),
      },
      {
        title: "状态",
        dataIndex: "lastStatus",
        width: 100,
        render: (value: string) => (
          <Tag color={statusMeta[value]?.color ?? "default"}>{statusMeta[value]?.label ?? value}</Tag>
        ),
      },
      {
        title: "CPU / 连接",
        width: 150,
        render: (_, row) => {
          if (row.targetType === "server") {
            return <MetricProgress value={summaryNumber(row.latestSnapshot, "cpuUsagePercent")} />;
          }
          const value =
            row.targetType === "redis"
              ? summaryNumber(row.latestSnapshot, "connectedClients")
              : summaryNumber(row.latestSnapshot, "activeConnections");
          return <Text>{value === null ? "-" : value.toFixed(0)}</Text>;
        },
      },
      {
        title: "内存 / 缓存",
        width: 150,
        render: (_, row) => {
          if (row.targetType === "server") {
            return <MetricProgress value={summaryNumber(row.latestSnapshot, "memoryUsagePercent")} />;
          }
          if (row.targetType === "redis") {
            const memoryUsage = summaryNumber(row.latestSnapshot, "memoryUsagePercent");
            const usedMemory = summaryNumber(row.latestSnapshot, "usedMemoryBytes");
            return memoryUsage === null ? (
              <Text>{formatStaticBytes(usedMemory)}</Text>
            ) : (
              <MetricProgress value={memoryUsage} />
            );
          }
          const cacheHit = summaryNumber(row.latestSnapshot, "cacheHitPercent");
          return <Text>{cacheHit === null ? "-" : `${cacheHit.toFixed(1)}%`}</Text>;
        },
      },
      {
        title: "磁盘 / 容量",
        width: 150,
        render: (_, row) => {
          if (row.targetType === "server") {
            return <MetricProgress value={summaryNumber(row.latestSnapshot, "diskUsagePercent")} />;
          }
          const bytes =
            row.targetType === "redis"
              ? summaryNumber(row.latestSnapshot, "keyCount")
              : summaryNumber(row.latestSnapshot, "databaseSizeBytes");
          if (bytes === null) return <Text type="secondary">-</Text>;
          return <Text>{row.targetType === "redis" ? `${bytes.toFixed(0)} keys` : formatStaticBytes(bytes)}</Text>;
        },
      },
      {
        title: "网络 / 状态",
        width: 160,
        render: (_, row) => {
          if (row.targetType !== "server") {
            if (row.targetType === "redis") {
              const hit = summaryNumber(row.latestSnapshot, "hitPercent");
              const slowlog = summaryNumber(row.latestSnapshot, "slowlogLen");
              return (
                <Space direction="vertical" size={0}>
                  <Text className="text-xs">命中 {formatPercent(hit)}</Text>
                  <Text className="text-xs">慢日志 {slowlog === null ? "-" : slowlog.toFixed(0)}</Text>
                </Space>
              );
            }
            const slow = summaryNumber(row.latestSnapshot, "slowQueries");
            const locks = summaryNumber(row.latestSnapshot, "lockWaits");
            return (
              <Space direction="vertical" size={0}>
                <Text className="text-xs">慢查询 {slow === null ? "-" : slow.toFixed(0)}</Text>
                <Text className="text-xs">锁等待 {locks === null ? "-" : locks.toFixed(0)}</Text>
              </Space>
            );
          }
          const rx = summaryNumber(row.latestSnapshot, "networkRxBytesPerSec");
          const tx = summaryNumber(row.latestSnapshot, "networkTxBytesPerSec");
          return (
            <Space direction="vertical" size={0}>
              <Text className="text-xs">RX {formatBytes(rx)}</Text>
              <Text className="text-xs">TX {formatBytes(tx)}</Text>
            </Space>
          );
        },
      },
      {
        title: "最近采集",
        dataIndex: "lastCollectedAt",
        width: 170,
        render: (value?: string | null) => value ?? "-",
      },
      {
        title: "操作",
        width: 180,
        fixed: "right",
        render: (_, row) => {
          const key = `${row.targetType}:${row.targetKey}`;
          return (
            <Space>
              <Button
                size="small"
                icon={<RefreshCw size={14} />}
                loading={collectingKey === key}
                onClick={() => collectTarget(row)}
              >
                刷新
              </Button>
              <Button size="small" onClick={() => openDetail(row)}>
                详情
              </Button>
            </Space>
          );
        },
      },
    ],
    [collectingKey],
  );

  const groupOptions = useMemo(() => {
    const groups = Array.from(new Set(targets.map((item) => item.groupName).filter(Boolean)));
    return [
      { label: "全部分组", value: "all" },
      ...groups.map((group) => ({ label: group, value: group })),
    ];
  }, [targets]);

  const filteredTargets = useMemo(() => {
    const normalizedKeyword = keyword.trim().toLowerCase();
    return targets.filter((item) => {
      if (typeFilter !== "all" && item.targetType !== typeFilter) return false;
      if (statusFilter !== "all" && item.lastStatus !== statusFilter) return false;
      if (groupFilter !== "all" && item.groupName !== groupFilter) return false;
      if (!normalizedKeyword) return true;
      return [item.displayName, item.targetKey, item.groupName, item.targetType]
        .join(" ")
        .toLowerCase()
        .includes(normalizedKeyword);
    });
  }, [groupFilter, keyword, statusFilter, targets, typeFilter]);

  const riskSummary = useMemo(() => {
    let highCpu = 0;
    let highMemory = 0;
    let highDisk = 0;
    let databaseWarning = 0;
    let redisRisk = 0;
    for (const target of targets) {
      const snapshot = target.latestSnapshot;
      if (!snapshot) continue;
      if (target.targetType === "server") {
        if ((summaryNumber(snapshot, "cpuUsagePercent") ?? 0) >= 85) highCpu += 1;
        if ((summaryNumber(snapshot, "memoryUsagePercent") ?? 0) >= 85) highMemory += 1;
        if ((summaryNumber(snapshot, "diskUsagePercent") ?? 0) >= 85) highDisk += 1;
      } else if (target.targetType === "redis") {
        if ((summaryNumber(snapshot, "memoryUsagePercent") ?? 0) >= 85 || (summaryNumber(snapshot, "slowlogLen") ?? 0) > 0) {
          redisRisk += 1;
        }
      } else if (
        (summaryNumber(snapshot, "connectionUsagePercent") ?? 0) >= 80 ||
        (summaryNumber(snapshot, "lockWaits") ?? 0) > 0 ||
        (summaryNumber(snapshot, "slowQueries") ?? 0) > 0
      ) {
        databaseWarning += 1;
      }
    }
    return { highCpu, highMemory, highDisk, databaseWarning, redisRisk };
  }, [targets]);

  const selectedSnapshot = selected?.latestSnapshot;
  const historyColumns = useMemo(
    () => buildHistoryColumns(selected?.targetType ?? "server"),
    [selected?.targetType],
  );

  return (
    <div className="prototype-page">
      <div className="prototype-page-header">
        <div>
          <Title level={2} style={{ margin: 0, fontSize: 24, lineHeight: "32px" }}>
            资源监控
          </Title>
          <Paragraph type="secondary" style={{ margin: "6px 0 0" }}>
            复用已配置的 SSH 服务器、数据库和 Redis 连接，采集 CPU、内存、磁盘、网络和运行状态。
          </Paragraph>
        </div>
        <Space>
          <Button onClick={loadData} loading={loading} icon={<RefreshCw size={16} />}>
            刷新列表
          </Button>
          <Button type="primary" onClick={collectBatch} loading={collectingKey === "__batch__"}>
            批量采集
          </Button>
        </Space>
      </div>

      <div className="prototype-grid prototype-grid-4">
        <Card>
          <Statistic title="监控目标" value={overview?.totalTargets ?? 0} suffix={`/ ${overview?.enabledTargets ?? 0} 启用`} />
        </Card>
        <Card>
          <Statistic title="正常" value={overview?.healthyTargets ?? 0} prefix={<Activity size={18} />} />
        </Card>
        <Card>
          <Statistic title="预警" value={overview?.warningTargets ?? 0} prefix={<MemoryStick size={18} />} />
        </Card>
        <Card>
          <Statistic title="失败" value={overview?.failedTargets ?? 0} prefix={<HardDrive size={18} />} />
        </Card>
        <Card>
          <Statistic
            title="打开告警"
            value={overview?.openAlerts ?? 0}
            styles={{ content: { color: (overview?.openAlerts ?? 0) > 0 ? "#cf1322" : undefined } }}
          />
        </Card>
      </div>

      {batchResult ? (
        <Alert
          showIcon
          type={batchResult.failed > 0 ? "warning" : "success"}
          message={`最近一次批量采集：目标 ${batchResult.total}，成功 ${batchResult.success}，失败 ${batchResult.failed}`}
        />
      ) : null}

      <Card title="筛选">
        <Space wrap>
          <Select
            value={typeFilter}
            style={{ width: 150 }}
            onChange={setTypeFilter}
            options={[
              { label: "全部类型", value: "all" },
              { label: "服务器", value: "server" },
              { label: "MySQL", value: "mysql" },
              { label: "PostgreSQL", value: "postgresql" },
              { label: "Redis", value: "redis" },
            ]}
          />
          <Select
            value={statusFilter}
            style={{ width: 140 }}
            onChange={setStatusFilter}
            options={[
              { label: "全部状态", value: "all" },
              { label: "未采集", value: "unknown" },
              { label: "正常", value: "healthy" },
              { label: "预警", value: "warning" },
              { label: "失败", value: "failed" },
            ]}
          />
          <Select value={groupFilter} style={{ width: 150 }} onChange={setGroupFilter} options={groupOptions} />
          <Input.Search
            allowClear
            placeholder="搜索资源名称、Key 或分组"
            style={{ width: 260 }}
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
          />
          <Button
            onClick={() => {
              setTypeFilter("all");
              setStatusFilter("all");
              setGroupFilter("all");
              setKeyword("");
            }}
          >
            重置
          </Button>
        </Space>
      </Card>

      <div
        className="prototype-grid"
        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}
      >
        <Card>
          <Statistic title="CPU 高负载" value={riskSummary.highCpu} suffix="个" />
        </Card>
        <Card>
          <Statistic title="内存高使用" value={riskSummary.highMemory} suffix="个" />
        </Card>
        <Card>
          <Statistic title="磁盘高使用" value={riskSummary.highDisk} suffix="个" />
        </Card>
        <Card>
          <Statistic title="数据库状态风险" value={riskSummary.databaseWarning} suffix="个" />
        </Card>
        <Card>
          <Statistic title="Redis 风险" value={riskSummary.redisRisk} suffix="个" />
        </Card>
      </div>

      <Card title="资源列表">
        <Table
          rowKey={(row) => `${row.targetType}:${row.targetKey}`}
          columns={columns}
          dataSource={filteredTargets}
          loading={loading}
          scroll={{ x: 1220 }}
          pagination={{ pageSize: 10, showSizeChanger: false }}
        />
      </Card>

      <Card
        title="打开告警"
        extra={
          <Button size="small" onClick={loadData} loading={loading}>
            刷新
          </Button>
        }
      >
        <Table
          rowKey="id"
          size="small"
          dataSource={alertEvents}
          pagination={{ pageSize: 8, showSizeChanger: false }}
          columns={[
            {
              title: "级别",
              dataIndex: "severity",
              width: 90,
              render: (value) => (
                <Tag color={severityMeta[String(value)]?.color ?? "default"}>
                  {severityMeta[String(value)]?.label ?? String(value)}
                </Tag>
              ),
            },
            {
              title: "资源",
              width: 220,
              render: (_, row) => (
                <Space direction="vertical" size={0}>
                  <Text strong>{targetTypeMeta[row.targetType]?.label ?? row.targetType}</Text>
                  <Text type="secondary" className="text-xs">{row.targetKey}</Text>
                </Space>
              ),
            },
            { title: "指标", dataIndex: "metricKey", width: 170 },
            {
              title: "当前/阈值",
              width: 140,
              render: (_, row) => `${row.metricValue.toFixed(2)} / ${row.thresholdValue.toFixed(2)}`,
            },
            { title: "说明", dataIndex: "message", ellipsis: true },
            { title: "最近触发", dataIndex: "lastSeenAt", width: 170 },
            {
              title: "操作",
              width: 90,
              fixed: "right",
              render: (_, row) => (
                <Button size="small" onClick={() => resolveAlert(row.id)}>
                  解决
                </Button>
              ),
            },
          ]}
        />
      </Card>

      <Card
        title="阈值规则"
        extra={
          <Button type="primary" size="small" onClick={() => openRuleModal()}>
            新增规则
          </Button>
        }
      >
        <Table
          rowKey="id"
          size="small"
          dataSource={alertRules}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          columns={[
            {
              title: "资源类型",
              dataIndex: "targetType",
              width: 120,
              render: (value) => (
                <Tag color={targetTypeMeta[String(value)]?.color ?? "default"}>
                  {targetTypeMeta[String(value)]?.label ?? String(value)}
                </Tag>
              ),
            },
            {
              title: "目标",
              dataIndex: "targetKey",
              width: 180,
              render: (value) => (value === "*" ? "全部目标" : value),
            },
            { title: "指标", dataIndex: "metricKey", width: 180 },
            {
              title: "条件",
              width: 120,
              render: (_, row) => `${row.operator} ${row.thresholdValue}`,
            },
            {
              title: "级别",
              dataIndex: "severity",
              width: 90,
              render: (value) => (
                <Tag color={severityMeta[String(value)]?.color ?? "default"}>
                  {severityMeta[String(value)]?.label ?? String(value)}
                </Tag>
              ),
            },
            {
              title: "启用",
              dataIndex: "enabled",
              width: 90,
              render: (enabled, row) => (
                <Switch size="small" checked={enabled} onChange={(checked) => toggleRule(row, checked)} />
              ),
            },
            { title: "更新时间", dataIndex: "updatedAt", width: 170 },
            {
              title: "操作",
              width: 140,
              fixed: "right",
              render: (_, row) => (
                <Space>
                  <Button size="small" onClick={() => openRuleModal(row)}>
                    编辑
                  </Button>
                  <Popconfirm title="确认删除该规则？" onConfirm={() => deleteRule(row.id)}>
                    <Button size="small" danger>
                      删除
                    </Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      </Card>

      <Modal
        title={editingRule ? "编辑告警规则" : "新增告警规则"}
        open={ruleModalOpen}
        onCancel={() => setRuleModalOpen(false)}
        onOk={submitRule}
        destroyOnHidden
      >
        <Form form={ruleForm} layout="vertical" preserve={false}>
          <Form.Item name="targetType" label="资源类型" rules={[{ required: true, message: "请选择资源类型" }]}>
            <Select
              options={[
                { label: "服务器", value: "server" },
                { label: "MySQL", value: "mysql" },
                { label: "PostgreSQL", value: "postgresql" },
                { label: "Redis", value: "redis" },
              ]}
            />
          </Form.Item>
          <Form.Item name="targetKey" label="目标 Key">
            <Input placeholder="* 表示该类型全部目标；也可填写具体服务器别名或连接 Key" />
          </Form.Item>
          <Form.Item noStyle shouldUpdate={(prev, next) => prev.targetType !== next.targetType}>
            {({ getFieldValue }) => {
              const targetType = getFieldValue("targetType") || "server";
              return (
                <Form.Item name="metricKey" label="指标" rules={[{ required: true, message: "请选择指标" }]}>
                  <Select
                    showSearch
                    options={metricOptions
                      .filter((item) => item.targetTypes.includes(targetType))
                      .map((item) => ({ label: `${item.label} (${item.value})`, value: item.value }))}
                  />
                </Form.Item>
              );
            }}
          </Form.Item>
          <Space align="start" style={{ width: "100%" }}>
            <Form.Item name="operator" label="条件" rules={[{ required: true, message: "请选择条件" }]}>
              <Select
                style={{ width: 120 }}
                options={[">", ">=", "<", "<=", "=="].map((value) => ({ label: value, value }))}
              />
            </Form.Item>
            <Form.Item
              name="thresholdValue"
              label="阈值"
              rules={[{ required: true, message: "请输入阈值" }]}
            >
              <InputNumber style={{ width: 160 }} precision={2} />
            </Form.Item>
          </Space>
          <Form.Item name="severity" label="级别" rules={[{ required: true, message: "请选择级别" }]}>
            <Select
              options={[
                { label: "提示", value: "info" },
                { label: "警告", value: "warning" },
                { label: "严重", value: "critical" },
              ]}
            />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>

      <Drawer
        title={selected ? `${selected.displayName} 资源详情` : "资源详情"}
        open={!!selected}
        onClose={() => setSelected(null)}
        width={760}
      >
        {selected ? (
          <Space direction="vertical" size={16} style={{ width: "100%" }}>
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="资源类型">
                {targetTypeMeta[selected.targetType]?.label ?? selected.targetType}
              </Descriptions.Item>
              <Descriptions.Item label="资源 Key">{selected.targetKey}</Descriptions.Item>
              <Descriptions.Item label="状态">
                <Tag color={statusMeta[selected.lastStatus]?.color ?? "default"}>
                  {statusMeta[selected.lastStatus]?.label ?? selected.lastStatus}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="最近采集">{selected.lastCollectedAt ?? "-"}</Descriptions.Item>
              <Descriptions.Item label="耗时">{selectedSnapshot?.durationMs ?? "-"} ms</Descriptions.Item>
              <Descriptions.Item label="错误">{selected.lastError ?? "-"}</Descriptions.Item>
            </Descriptions>

            {renderDetailMetricCards(selected.targetType, selectedSnapshot)}

            {renderDetailStatusCard(selected.targetType, selectedSnapshot)}

            <Card title="指标趋势">
              <div
                className="prototype-grid"
                style={{ gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))" }}
              >
                {trendMetricConfigs(selected.targetType).map((item) => (
                  <TrendCard
                    key={item.key}
                    title={item.label}
                    snapshots={history}
                    metricKey={item.key}
                    formatter={item.formatter}
                  />
                ))}
              </div>
            </Card>

            <Card title="告警记录">
              <Table
                rowKey="id"
                size="small"
                dataSource={detailAlerts}
                pagination={{ pageSize: 5, showSizeChanger: false }}
                columns={[
                  {
                    title: "级别",
                    dataIndex: "severity",
                    width: 80,
                    render: (value) => (
                      <Tag color={severityMeta[String(value)]?.color ?? "default"}>
                        {severityMeta[String(value)]?.label ?? String(value)}
                      </Tag>
                    ),
                  },
                  {
                    title: "状态",
                    dataIndex: "status",
                    width: 90,
                    render: (value) => (
                      <Tag color={String(value) === "open" ? "red" : "green"}>
                        {String(value) === "open" ? "打开" : "已解决"}
                      </Tag>
                    ),
                  },
                  { title: "指标", dataIndex: "metricKey", width: 150 },
                  {
                    title: "当前/阈值",
                    width: 130,
                    render: (_, row) => `${row.metricValue.toFixed(2)} / ${row.thresholdValue.toFixed(2)}`,
                  },
                  { title: "最近触发", dataIndex: "lastSeenAt", width: 170 },
                  { title: "说明", dataIndex: "message", ellipsis: true },
                ]}
              />
            </Card>

            <Card title="快照历史">
              <Table
                rowKey="id"
                size="small"
                dataSource={history}
                pagination={{ pageSize: 8, showSizeChanger: false }}
                columns={historyColumns}
              />
            </Card>

            <Card title="原始指标 JSON">
              <pre style={{ maxHeight: 360, overflow: "auto", margin: 0 }}>
                {JSON.stringify(selectedSnapshot?.metrics ?? {}, null, 2)}
              </pre>
            </Card>
          </Space>
        ) : null}
      </Drawer>
    </div>
  );
}

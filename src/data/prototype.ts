import type { ReactNode } from "react";

export type RiskLevel = "L0" | "L1" | "L2" | "L3" | "readonly" | "blocked" | "ai";

export interface ServerRecord {
  alias: string;
  group: string;
  host: string;
  source: string;
  auth: string;
  policy: RiskLevel;
  status: string;
}

export interface ApprovalRecord {
  id: string;
  source: string;
  server: string;
  action: string;
  risk: RiskLevel;
  reason: string;
  grant: string;
}

export interface AuditRecord {
  time: string;
  actor: string;
  server: string;
  action: string;
  risk: RiskLevel;
  result: string;
  summary: string;
}

export interface CoverageRecord {
  feature: string;
  page: string;
  status: "覆盖" | "预留";
}

export interface NavItem {
  path: string;
  label: string;
  icon: ReactNode;
}

export const stats = [
  { label: "服务器", value: "28", hint: "7 个分组，3 个来源" },
  { label: "在线会话", value: "6", hint: "2 个 AI 辅助中" },
  { label: "待审批", value: "4", hint: "1 个 L3 高风险" },
  { label: "日志监听", value: "9", hint: "3 个标签暂停" },
];

export const servers: ServerRecord[] = [
  {
    alias: "prod-api-01",
    group: "生产 / API",
    host: "10.18.2.21:22",
    source: "manual",
    auth: "vault:key-prod",
    policy: "L2",
    status: "online",
  },
  {
    alias: "prod-worker-02",
    group: "生产 / Worker",
    host: "10.18.2.32:22",
    source: "~/.ssh/config",
    auth: "vault:key-prod",
    policy: "L1",
    status: "online",
  },
  {
    alias: "stage-app-01",
    group: "预发",
    host: "172.20.8.12:22",
    source: "manual",
    auth: "vault:password-stage",
    policy: "L2",
    status: "degraded",
  },
  {
    alias: "sgcc-jump-01",
    group: "堡垒机",
    host: "isc.jumpserver/session",
    source: "jumpserver",
    auth: "session reference",
    policy: "blocked",
    status: "web",
  },
];

export const approvals: ApprovalRecord[] = [
  {
    id: "APR-1008",
    source: "AI 面板",
    server: "stage-app-01",
    action: "写入 /opt/app/app.yml",
    risk: "L2",
    reason: "用户要求调整日志级别",
    grant: "允许同类 10 分钟",
  },
  {
    id: "APR-1009",
    source: "终端",
    server: "prod-api-01",
    action: "restart nginx",
    risk: "L2",
    reason: "服务发布后 reload",
    grant: "单次允许",
  },
  {
    id: "APR-1010",
    source: "MCP Client",
    server: "prod-log-02",
    action: "清理日志目录",
    risk: "L3",
    reason: "磁盘空间不足",
    grant: "已拦截，需改为归档方案",
  },
];

export const logTabs = [
  {
    key: "api",
    title: "prod-api-01:/opt/app/logs/app.log",
    status: "running",
    lines: [
      "10:41:22 INFO request_id=8df checkout started user=***",
      "10:41:28 ERROR payment upstream timeout endpoint=/pay/commit",
      "10:41:30 WARN retry scheduled attempt=2 latency=1800ms",
      "10:41:33 ERROR payment upstream timeout endpoint=/pay/commit",
    ],
  },
  {
    key: "worker",
    title: "prod-worker-02:/opt/worker/logs/job.log",
    status: "paused",
    lines: [
      "10:39:01 INFO job sync-order started",
      "10:39:03 INFO batch size=500 duration=812ms",
    ],
  },
  {
    key: "stage",
    title: "stage-app-01:/opt/app/logs/app.log",
    status: "reconnecting",
    lines: [
      "10:40:12 WARN ssh tunnel reconnecting",
      "10:40:18 INFO reconnect attempt=2",
    ],
  },
];

export const files = [
  { name: "app.yml", type: "YAML", size: "6.4 KB", modified: "今日 10:22", permission: "可写需审批" },
  { name: "logs", type: "Directory", size: "-", modified: "今日", permission: "可读" },
  { name: "deploy.sh", type: "Shell", size: "3.2 KB", modified: "昨日", permission: "执行需审批" },
  { name: "nginx.conf", type: "Conf", size: "9.1 KB", modified: "周一", permission: "只读" },
];

export const mcpTools = [
  { tool: "list_servers", policy: "只读允许", audit: "记录调用者和返回数量" },
  { tool: "system_info", policy: "只读确认", audit: "记录服务器和摘要" },
  { tool: "ssh_exec", policy: "按风险审批", audit: "记录命令和脱敏输出摘要" },
  { tool: "tail_log", policy: "只读确认", audit: "记录文件路径和过滤条件" },
  { tool: "sftp_list", policy: "只读允许", audit: "记录路径" },
  { tool: "sftp_write", policy: "写入审批", audit: "记录差异摘要和审批链路" },
];

export const auditRows: AuditRecord[] = [
  {
    time: "10:42:13",
    actor: "mcp_client: Codex",
    server: "prod-api-01",
    action: "ssh_exec",
    risk: "L0",
    result: "exit 0",
    summary: "du 分析 /var 使用量",
  },
  {
    time: "10:39:51",
    actor: "ai",
    server: "stage-app-01",
    action: "sftp_write",
    risk: "L2",
    result: "审批通过",
    summary: "app.yml 保存，凭据字段已脱敏",
  },
  {
    time: "10:36:28",
    actor: "user",
    server: "prod-api-01",
    action: "tail_search_filter",
    risk: "readonly",
    result: "成功",
    summary: "搜索 ERROR，过滤开启，2 个匹配",
  },
  {
    time: "10:31:04",
    actor: "system",
    server: "sgcc-jump-01",
    action: "jumpserver_session",
    risk: "blocked",
    result: "建议-only",
    summary: "只打开 Web SSH，不提取凭据",
  },
];

export const coverageRows: CoverageRecord[] = [
  { feature: "启动引导", page: "01 启动引导", status: "覆盖" },
  { feature: "服务器分组管理", page: "03 服务器管理", status: "覆盖" },
  { feature: "~/.ssh/config 导入", page: "05 SSH Config 导入", status: "覆盖" },
  { feature: "SQLite 加密凭据字段", page: "06 凭据保险库", status: "覆盖" },
  { feature: "命令行 AI 问答", page: "07 终端 + AI", status: "覆盖" },
  { feature: "AI 权限控制", page: "08 审批队列", status: "覆盖" },
  { feature: "多标签日志监听", page: "09 日志监听", status: "覆盖" },
  { feature: "日志搜索与过滤", page: "09 日志监听", status: "覆盖" },
  { feature: "SFTP 上传下载", page: "10 SFTP 文件", status: "覆盖" },
  { feature: "SFTP 内置文本编辑器", page: "11 文本编辑器", status: "覆盖" },
  { feature: "多 AI Provider", page: "12 AI Provider", status: "覆盖" },
  { feature: "本应用作为 MCP Server", page: "13 MCP Server", status: "覆盖" },
  { feature: "JumpServer 会话兼容", page: "14 堡垒机会话", status: "覆盖" },
  { feature: "审计日志", page: "15 审计日志", status: "覆盖" },
  { feature: "团队字段预留", page: "16 团队预留", status: "覆盖" },
  { feature: "空状态和错误状态", page: "18 状态页", status: "覆盖" },
  { feature: "macOS / Windows 首发", page: "17 系统设置", status: "覆盖" },
];

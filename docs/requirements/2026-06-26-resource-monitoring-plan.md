# 服务器与数据库资源使用状态监控功能实施方案

**状态**: 规划中
**创建时间**: 2026-06-26
**目标版本**: v0.1.x / v0.2
**目标模块**: 工作台 / 服务器管理 / 数据库管理 / Redis 管理 / MCP Server

---

## 1. 背景与目标

当前应用已经具备服务器资产管理、SSH 终端、SFTP、日志监听、数据库管理、Redis 浏览、AI 权限策略、审批队列、审计日志和 MCP Server。下一步如果要展示服务器和数据库的 CPU、内存、IO、网络、磁盘等资源使用状态，核心原则应该是复用已有连接和安全体系，而不是另起一套监控平台。

目标是让用户在本应用内看到以下资源状态：

- SSH 服务器：CPU、内存、Swap、磁盘分区、磁盘 IO、网络吞吐、负载、进程 Top、端口占用。
- MySQL / PostgreSQL：连接数、活动会话、慢查询趋势、锁等待、QPS/TPS、缓存命中、库/表空间、复制状态。
- Redis：内存、客户端连接、命令吞吐、命中率、Key 数、过期/淘汰、持久化状态、慢日志。
- 工作台总览：异常资源、离线目标、阈值告警、最近采集时间。
- AI/MCP：允许 Agent 只读读取资源摘要，生成诊断建议；任何修复动作继续走已有 AI 权限、审批和审计。

一句话定位：

> 一个基于 SSH 和数据库驱动的轻量资源观测中心，优先做实时诊断与近期趋势，不做重型 Prometheus 替代品。

---

## 2. 方案对比

### 2.1 方案 A：Agentless 采集，复用 SSH 与数据库连接

通过现有 SSH 服务器配置执行只读系统命令，通过已有数据库连接执行只读状态查询，通过 Redis `INFO` / `DBSIZE` / `SCAN` 获取状态。

优点：

- 不需要在服务器安装 Agent，落地最快。
- 复用现有服务器、数据库、凭据、AI 权限和审计。
- 对内网服务器友好，只要 SSH 或数据库连接可用即可。
- macOS / Windows 桌面端实现一致，采集逻辑在 Rust 后端。

缺点：

- 高频采集不适合，SSH 执行命令有开销。
- 部分 IO 指标依赖目标机器安装 `iostat` / `sar`，需要降级方案。
- 无法采集没有 SSH 关联的数据库主机系统指标。

适用阶段：v0.1 首选。

### 2.2 方案 B：安装轻量 Agent

在每台服务器部署一个应用专属 Agent，Agent 主动上报指标或等待桌面端拉取。

优点：

- 采集稳定、指标更全、频率更高。
- 可做日志、进程、端口、文件变更等扩展。
- 不依赖每次 SSH 执行命令。

缺点：

- 安装、升级、权限、卸载和安全审计成本高。
- 企业内网环境可能被安全软件拦截。
- 首版会显著拉高复杂度。

适用阶段：v0.3 以后作为可选增强，不作为首版方案。

### 2.3 方案 C：接入 Prometheus / Node Exporter / Grafana

如果用户已有监控系统，应用只做数据源读取和展示。

优点：

- 指标专业、历史数据完整。
- 不重复造监控基础设施。

缺点：

- 依赖外部平台，不适合个人桌面工具首版。
- 需要额外配置 Token、地址、指标名映射。

适用阶段：v0.2+ 可选数据源。

### 2.4 推荐结论

首版采用 **方案 A：Agentless 采集**。

实现策略：

1. 服务器资源通过 SSH 只读命令采集。
2. MySQL / PostgreSQL / Redis 通过已有数据库连接读取运行状态。
3. 如果数据库连接绑定了 SSH 隧道服务器，则可以同时展示“数据库实例状态 + 所在服务器资源状态”。
4. 指标保存到本地 SQLite，保留近期原始数据和轻量聚合数据。
5. 告警、AI 解释、MCP 工具都只消费同一套 Service 层数据。

---

## 3. 功能范围

### 3.1 v0.1 必做

#### 服务器资源

- 手动刷新单台服务器资源状态。
- 批量刷新已启用服务器资源状态。
- 支持 CPU 使用率、系统负载、内存、Swap、磁盘分区、磁盘 IO、网络吞吐、运行时间。
- 支持进程 Top 列表：CPU Top、内存 Top。
- 支持端口监听摘要。
- 支持采集失败原因展示。
- 支持最近一次采集结果和最近 1 小时趋势。

#### MySQL / PostgreSQL

- 手动刷新单个数据库连接状态。
- 数据库连接数、活动会话、锁等待、库大小、表大小、缓存命中率。
- MySQL 额外支持 QPS、Threads、InnoDB Buffer Pool、慢查询计数。
- PostgreSQL 额外支持 `pg_stat_activity`、事务提交/回滚、deadlocks、blocks hit/read。
- 数据库指标必须支持直连和 SSH 隧道连接。

#### Redis

- Redis `INFO` 状态采集。
- 内存使用、连接数、命令吞吐、keyspace hits/misses、expired/evicted keys。
- DB 维度 Key 数、平均 TTL 可选。
- 慢日志摘要。

#### 页面

- 工作台新增“资源状态”摘要卡片。
- 服务器管理列表显示 CPU、内存、磁盘、最近采集时间。
- 数据库管理页面新增“资源状态”Tab。
- 新增统一“资源监控”页面，展示服务器、数据库、Redis 三类资源。

#### 安全与审计

- 所有采集命令必须是只读命令。
- 每次手动采集写入审计日志。
- 定时后台采集只记录摘要审计，避免刷屏。
- AI/MCP 读取指标必须脱敏连接信息，不返回密码、私钥、Token。

### 3.2 v0.2 应做

- 定时采集任务：按服务器/数据库配置采集周期。
- 阈值规则：CPU、内存、磁盘、连接数、慢查询、Redis 内存等。
- 告警列表与已读/忽略。
- 资源趋势图：最近 1 小时、6 小时、24 小时。
- 导出资源报告 Markdown / CSV。
- AI 一键诊断：基于当前指标、日志片段、数据库状态生成诊断摘要。
- MCP 工具开放：
  - `resource_targets_list`
  - `server_resource_snapshot`
  - `database_resource_snapshot`
  - `redis_resource_snapshot`
  - `resource_alerts_list`

### 3.3 暂不做

- 不做秒级实时监控。
- 不做完整 Prometheus 时序数据库。
- 不做 Agent 强制安装。
- 不做绕过堡垒机或安全策略的采集方式。
- 不保存敏感凭证明文。

---

## 4. 指标采集设计

### 4.1 服务器 Linux 指标

首版目标服务器以 Linux 为主，通过 SSH 执行只读命令。

| 指标 | 首选方式 | 降级方式 | 说明 |
| --- | --- | --- | --- |
| CPU | `/proc/stat` 两次采样差值 | `top -bn1` | 推荐后端采集两次，间隔 800ms |
| Load | `/proc/loadavg` | `uptime` | 1/5/15 分钟负载 |
| 内存 | `/proc/meminfo` | `free -b` | total/used/free/cache/available |
| Swap | `/proc/meminfo` | `free -b` | total/used |
| 磁盘分区 | `df -P -B1` | `df -hP` | path、fs、used、available、usage |
| 磁盘 IO | `/proc/diskstats` 两次采样 | `iostat -dx 1 1` | read/write bytes、util 近似 |
| 网络 | `/proc/net/dev` 两次采样 | `ip -s link` | rx/tx bytes、packets、errors |
| 进程 Top | `ps -eo pid,comm,pcpu,pmem,rss --sort=-pcpu` | `top -bn1` | 限制前 10/20 |
| 端口 | `ss -tulpen` | `netstat -tulpen` | 只读摘要 |
| 系统信息 | `uname -a`、`uptime -s` | `hostnamectl` | OS、内核、启动时间 |

采集命令必须通过统一白名单管理，不能拼接用户输入形成任意命令。

### 4.2 MySQL 指标

通过数据库驱动执行只读 SQL。

| 指标 | SQL 来源 |
| --- | --- |
| 版本 | `SELECT VERSION()` |
| 连接数 | `SHOW GLOBAL STATUS LIKE 'Threads_connected'` |
| 最大连接数 | `SHOW VARIABLES LIKE 'max_connections'` |
| QPS 粗略值 | `Questions` / 时间差 |
| 慢查询 | `SHOW GLOBAL STATUS LIKE 'Slow_queries'` |
| InnoDB 缓冲池 | `SHOW GLOBAL STATUS LIKE 'Innodb_buffer_pool_%'` |
| 锁等待 | `information_schema.innodb_trx` / `performance_schema` 可用时 |
| 库大小 | `information_schema.tables` 聚合 |
| 表大小 | `information_schema.tables` |
| 进程列表 | `SHOW PROCESSLIST` |

降级策略：

- 如果 `performance_schema` 不可用，只展示基础 `SHOW STATUS` 指标。
- 如果账号权限不足，页面显示“权限不足，无法读取该指标”，但不影响其他指标。

### 4.3 PostgreSQL 指标

通过数据库驱动执行只读 SQL。

| 指标 | SQL 来源 |
| --- | --- |
| 版本 | `SELECT version()` |
| 活动会话 | `pg_stat_activity` |
| 库大小 | `pg_database_size(current_database())` |
| 表大小 | `pg_total_relation_size` |
| 命中率 | `pg_stat_database.blks_hit / (blks_hit + blks_read)` |
| 事务 | `pg_stat_database.xact_commit / xact_rollback` |
| deadlocks | `pg_stat_database.deadlocks` |
| 锁 | `pg_locks` |
| 后台写入 | `pg_stat_bgwriter` |

降级策略：

- 普通账号无法读取完整 `pg_stat_activity` 时，使用当前用户可见行。
- 部分版本差异字段不存在时，按可用字段输出。

### 4.4 Redis 指标

通过 Redis 协议读取。

| 指标 | 命令 |
| --- | --- |
| 基础状态 | `INFO server` |
| 内存 | `INFO memory` |
| 客户端 | `INFO clients` |
| 命令统计 | `INFO commandstats` |
| 命中率 | `INFO stats` 中 `keyspace_hits/misses` |
| Key 数 | `INFO keyspace` |
| 慢日志 | `SLOWLOG LEN` / `SLOWLOG GET 10` |
| 当前 DB Key 数 | `DBSIZE` |

Redis 禁止首版做高风险命令采集，例如 `MONITOR`、`KEYS *`、`FLUSH*`。

---

## 5. 后端架构设计

### 5.1 模块拆分

建议新增模块：

```text
src-tauri/src/
├── commands/
│   └── resource_monitor.rs
├── services/
│   ├── resource_monitor.rs
│   └── collectors/
│       ├── mod.rs
│       ├── server_metrics.rs
│       ├── mysql_metrics.rs
│       ├── postgres_metrics.rs
│       └── redis_metrics.rs
├── models/
│   └── mod.rs
└── database/
    ├── mod.rs
    └── schema.rs
```

职责：

- `commands/resource_monitor.rs`：IPC 入口，只做参数校验和调用 Service。
- `services/resource_monitor.rs`：编排采集、阈值判断、审计、快照落库。
- `services/collectors/*`：不同资源类型的指标采集器。
- `database/mod.rs`：本地 SQLite CRUD。
- `models/mod.rs`：Rust 与 TypeScript 对齐的数据结构。

### 5.2 调用链

```text
React 资源监控页面
  -> src/lib/api/resourceMonitor.ts
  -> Tauri Command
  -> ResourceMonitorService
  -> Server / MySQL / PostgreSQL / Redis Collector
  -> AuditService / ApprovalService
  -> SQLite 快照与告警表
```

### 5.3 采集策略

首版建议同时支持：

1. **手动采集**
   - 用户点击刷新。
   - 返回最新快照。
   - 写入审计。

2. **进入页面自动采集**
   - 页面打开时对当前选择资源采集一次。
   - 有 30 秒内快照则优先使用缓存，避免频繁 SSH。

3. **后台定时采集**
   - v0.2 启用。
   - 使用 Rust `tokio::spawn` 后台任务。
   - 每个目标有独立采集周期和超时。
   - 应用退出时任务停止。

### 5.4 超时与并发

- 单台服务器采集超时：默认 8 秒。
- 数据库状态采集超时：默认 5 秒。
- Redis 状态采集超时：默认 3 秒。
- 批量刷新并发限制：默认 4 个目标。
- 失败不影响其他目标。

---

## 6. SQLite 表设计

建议从现有 schema 版本继续递增，新增以下表。

### 6.1 监控目标表

```sql
CREATE TABLE IF NOT EXISTS resource_monitor_targets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_type TEXT NOT NULL,
    target_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    collect_interval_sec INTEGER NOT NULL DEFAULT 60,
    last_status TEXT NOT NULL DEFAULT 'unknown',
    last_collected_at TEXT DEFAULT NULL,
    last_error TEXT DEFAULT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(target_type, target_key)
);
```

说明：

- `target_type`: `server` / `mysql` / `postgresql` / `redis`
- `target_key`: 服务器 alias 或数据库连接 key。

### 6.2 指标快照表

```sql
CREATE TABLE IF NOT EXISTS resource_metric_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_type TEXT NOT NULL,
    target_key TEXT NOT NULL,
    status TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    summary_json TEXT NOT NULL,
    metrics_json TEXT NOT NULL,
    error TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_resource_metric_target_time
ON resource_metric_snapshots(target_type, target_key, collected_at DESC);
```

说明：

- `summary_json` 保存页面常用摘要，便于列表快速渲染。
- `metrics_json` 保存完整指标，避免首版设计过多稀疏字段。
- 后续性能不足时再拆分时序明细表。

### 6.3 阈值规则表

```sql
CREATE TABLE IF NOT EXISTS resource_alert_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_type TEXT NOT NULL,
    target_key TEXT NOT NULL DEFAULT '*',
    metric_key TEXT NOT NULL,
    operator TEXT NOT NULL,
    threshold_value REAL NOT NULL,
    severity TEXT NOT NULL DEFAULT 'warning',
    duration_sec INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

示例规则：

- `server.cpu.usage_percent > 85`
- `server.memory.usage_percent > 90`
- `server.disk.root_usage_percent > 85`
- `mysql.connections.usage_percent > 80`
- `redis.memory.usage_percent > 85`

### 6.4 告警事件表

```sql
CREATE TABLE IF NOT EXISTS resource_alert_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id INTEGER NOT NULL,
    target_type TEXT NOT NULL,
    target_key TEXT NOT NULL,
    severity TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    metric_key TEXT NOT NULL,
    metric_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    message TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    resolved_at TEXT DEFAULT NULL,
    snapshot_id INTEGER DEFAULT NULL
);
```

### 6.5 数据保留策略

首版默认：

- 原始快照保留 7 天。
- 告警事件保留 90 天。
- 每个目标最多保留 5000 条快照。
- 清理任务在应用启动和采集完成后低频触发。

---

## 7. Command / API 设计

### 7.1 Tauri Commands

```rust
list_resource_monitor_targets(input) -> Vec<ResourceMonitorTarget>
upsert_resource_monitor_target(input) -> ResourceMonitorTarget
delete_resource_monitor_target(id) -> ()

collect_server_resource_snapshot(alias) -> ResourceSnapshot
collect_database_resource_snapshot(connection_key) -> ResourceSnapshot
collect_redis_resource_snapshot(connection_key, database) -> ResourceSnapshot
collect_resource_snapshots_batch(input) -> BatchResourceCollectResult

get_latest_resource_snapshot(target_type, target_key) -> Option<ResourceSnapshot>
list_resource_metric_snapshots(input) -> ResourceSnapshotPage
list_resource_alert_rules(input) -> Vec<ResourceAlertRule>
upsert_resource_alert_rule(input) -> ResourceAlertRule
delete_resource_alert_rule(id) -> ()
list_resource_alert_events(input) -> ResourceAlertEventPage
resolve_resource_alert_event(id) -> ()
```

### 7.2 TypeScript API

新增：

```text
src/types/resourceMonitor.ts
src/lib/api/resourceMonitor.ts
```

前端统一通过 `resourceMonitorApi` 调用，禁止页面裸写 `invoke()`。

### 7.3 错误返回

错误需要区分：

- `target_not_found`：目标不存在。
- `connection_failed`：SSH/数据库连接失败。
- `permission_denied`：数据库账号权限不足。
- `collector_timeout`：采集超时。
- `unsupported_metric`：当前系统或数据库版本不支持。

页面展示时用中文可读提示，不直接暴露底层命令中的敏感参数。

---

## 8. 前端页面设计

### 8.1 工作台

新增资源摘要卡片：

- 在线服务器数 / 异常服务器数。
- 数据库连接健康数。
- Redis 异常数。
- 当前打开告警数。
- 最近采集时间。

### 8.2 资源监控页面

建议新增菜单：

```text
运维
└── 资源监控
```

页面结构：

1. 顶部筛选
   - 资源类型：全部 / 服务器 / MySQL / PostgreSQL / Redis
   - 分组
   - 状态
   - 关键字
   - 刷新全部

2. 总览卡片
   - CPU 高负载目标
   - 内存高使用目标
   - 磁盘高使用目标
   - 数据库连接异常
   - Redis 内存风险

3. 资源列表
   - 名称
   - 类型
   - 状态
   - CPU
   - 内存
   - 磁盘
   - IO
   - 网络
   - 连接数
   - 最近采集
   - 操作：刷新 / 详情 / AI 诊断

4. 详情抽屉
   - 基础信息
   - 指标趋势
   - 进程 / 会话 / 慢日志
   - 告警记录
   - 原始指标 JSON

### 8.3 服务器详情嵌入

服务器管理页面增加“资源”入口或详情区：

- CPU / 内存 / 磁盘进度条。
- 最近 10 个进程 Top。
- 网络 RX/TX。
- 磁盘分区表。

### 8.4 数据库管理嵌入

数据库管理页面新增 `资源状态` Tab：

- MySQL / PostgreSQL 展示数据库状态。
- Redis 展示 Redis 状态。
- 如果连接绑定 SSH 隧道，展示关联服务器资源摘要。
- 支持“AI 分析当前状态”按钮。

### 8.5 图表组件建议

首版如果不新增依赖：

- 用 Ant Design `Statistic`、`Progress`、`Table`、`Tag`、`Descriptions` 实现摘要。
- 趋势图可先用轻量 SVG/Canvas 自绘 Sparkline。

如果允许新增依赖：

- 推荐 `echarts` 或 `@ant-design/plots`。
- 指标趋势、网络吞吐、连接数变化会更清晰。

---

## 9. AI 与 MCP 设计

### 9.1 AI 诊断

新增 AI 场景：`resource_monitor_ai`。

输入上下文：

- 当前目标基本信息。
- 最近一次快照摘要。
- 最近 N 条趋势点。
- 当前打开告警。
- 数据库/Redis 状态摘要。
- 可选日志片段或慢查询摘要。

AI 输出：

- 异常摘要。
- 可能原因。
- 建议排查命令或 SQL。
- 风险等级。
- 是否需要审批执行修复动作。

注意：

- AI 不能直接执行写入修复。
- 只读诊断命令可按服务器 AI 权限级别自动执行。
- 写操作仍走审批队列。

### 9.2 MCP 工具建议

第一批只读工具：

```text
resource_targets_list
server_resource_snapshot
database_resource_snapshot
redis_resource_snapshot
resource_alerts_list
resource_metric_history
```

第二批受控工具：

```text
resource_collect_now
resource_ai_diagnose
resource_report_generate
```

第三批审批工具：

```text
resource_remediation_plan_create
resource_remediation_execute_approved
```

所有 MCP 工具要求：

- 不返回凭证明文。
- 记录调用方、目标、结果数量和耗时。
- 资源采集类工具必须限流，避免 Agent 高频调用压垮目标机器。

---

## 10. 安全与权限

### 10.1 服务器采集命令白名单

允许命令只包含：

- `cat /proc/...`
- `df`
- `ps`
- `ss` / `netstat`
- `uname`
- `uptime`
- `free`
- `iostat` 可选

禁止：

- 带写入重定向的命令。
- `rm`、`kill`、`systemctl restart`、`service restart`。
- 任意用户输入拼接到 shell 中。

### 10.2 数据库只读边界

MySQL/PostgreSQL 只允许状态查询 SQL：

- `SELECT`
- `SHOW`
- `DESCRIBE`
- `EXPLAIN`

Redis 只允许：

- `INFO`
- `DBSIZE`
- `SLOWLOG LEN`
- `SLOWLOG GET`
- `SCAN` 带 count 限制
- `TTL`
- `TYPE`

### 10.3 审计事件

新增审计事件类型：

- `resource.collect.server`
- `resource.collect.database`
- `resource.collect.redis`
- `resource.alert.open`
- `resource.alert.resolve`
- `resource.ai.diagnose`
- `mcp.resource.read`

审计字段：

- 目标类型、目标 key、耗时、状态、错误摘要。
- 不记录密码、私钥、Token。
- 不记录完整大 JSON，只记录摘要和快照 ID。

---

## 11. 实施步骤

### 阶段 1：基础模型与快照存储

- [ ] 新增 Rust/TS 类型：目标、快照、告警规则、告警事件。
- [ ] 新增 SQLite 迁移表。
- [ ] 新增 `ResourceMonitorService` 和基础 Commands。
- [ ] 实现快照保存、查询、分页。
- [ ] 接入审计日志。

验收：

- 可以创建/查询监控目标。
- 可以保存一条模拟快照并在前端展示。

### 阶段 2：服务器 Agentless 采集

- [ ] 实现 SSH 只读命令采集器。
- [ ] 解析 `/proc/stat`、`/proc/meminfo`、`df`、`/proc/net/dev`。
- [ ] 实现进程 Top 和端口摘要。
- [ ] 支持单台手动刷新。
- [ ] 支持批量刷新并发限制。

验收：

- 服务器页面可看到 CPU、内存、磁盘、网络。
- 采集失败有明确错误。

### 阶段 3：MySQL/PostgreSQL/Redis 采集

- [ ] MySQL 状态查询。
- [ ] PostgreSQL 状态查询。
- [ ] Redis `INFO` 状态查询。
- [ ] 处理权限不足和版本差异。
- [ ] 数据库管理页新增资源状态 Tab。

验收：

- 数据库连接页可看到连接数、库大小、缓存命中等指标。
- Redis 可看到内存、Key 数、命中率。

### 阶段 4：统一资源监控页面

- [ ] 新增资源监控菜单。
- [ ] 实现资源列表、筛选、刷新、详情抽屉。
- [ ] 工作台添加资源摘要。
- [ ] 服务器/数据库页面嵌入资源摘要。

验收：

- 用户能从工作台快速定位异常资源。
- 每类资源都有详情入口。

### 阶段 5：阈值告警

- [ ] 阈值规则 CRUD。
- [ ] 采集后自动评估规则。
- [ ] 告警事件列表。
- [ ] 告警解决/忽略。

验收：

- CPU/内存/磁盘超阈值能生成告警。
- 告警能在工作台和资源监控页面看到。

### 阶段 6：AI/MCP 接入

- [ ] 新增资源诊断 AI prompt 构造。
- [ ] MCP 只读资源工具。
- [ ] 审计 MCP 调用。
- [ ] 限流与脱敏。

验收：

- AI 能基于指标给出诊断建议。
- MCP Agent 能读取资源摘要但无法读取凭据。

---

## 12. 验收标准

### 功能验收

- 服务器、MySQL、PostgreSQL、Redis 均可手动刷新资源状态。
- 同一个页面能查看最近快照和近期趋势。
- 采集失败不会导致页面崩溃。
- Redis 不使用 `KEYS *`。
- 数据库状态查询不会执行写 SQL。
- 审计日志可查到手动刷新和 AI/MCP 诊断记录。

### 性能验收

- 单台服务器手动采集一般不超过 8 秒。
- Redis 状态采集不超过 3 秒。
- 批量刷新 20 个资源时页面不冻结。
- 快照表 10 万行以内查询分页可接受。

### 安全验收

- 前端和 MCP 返回值不包含数据库密码、SSH 密码、私钥。
- 采集命令全部来自白名单。
- MCP 工具调用被审计。
- Agent 无法通过资源监控工具执行任意命令。

---

## 13. 风险与对策

| 风险 | 影响 | 对策 |
| --- | --- | --- |
| 目标机器缺少 `iostat` | IO 指标缺失 | 优先解析 `/proc/diskstats`，没有则显示不支持 |
| 数据库账号权限不足 | 部分指标缺失 | 指标级降级，不让整个采集失败 |
| SSH 批量采集慢 | 页面等待久 | 并发限制、缓存、超时、后台刷新 |
| 快照数据增长快 | SQLite 变大 | 保留策略、最大行数、定期清理 |
| 指标口径不同 | 用户误解 | 页面展示采集来源和更新时间 |
| AI 误判 | 错误建议 | AI 只诊断，不自动修复；修复动作走审批 |

---

## 14. 推荐首轮最小可交付

建议第一轮只做最小闭环：

1. 新增资源监控模型和 SQLite 表。
2. 实现服务器单台手动采集。
3. 在服务器管理页面展示 CPU、内存、磁盘、网络和最近采集时间。
4. 写入审计日志。
5. 前端增加“刷新资源”按钮和详情抽屉。

完成后再扩展数据库、Redis、告警、MCP 和 AI 诊断。这样风险最低，也能最快验证采集链路是否稳定。

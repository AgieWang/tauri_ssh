# ADR-001：团队知识库本地索引技术基线

- 状态：部分验证；本地默认模型已在 macOS 真实推理基准中选定，跨平台、禅道与远程验收待补齐
- 日期：2026-07-30
- 关联变更：`add-team-knowledge-base-rag`
- 基准环境：Apple Silicon macOS，Rust stable，`rusqlite 0.31.0` bundled SQLite 3.45.0

## 背景

团队知识库首期需要在不部署服务器向量数据库的情况下，完成中文需求、版本号、代码标识符、路径、API、SQL 与跨项目语义检索。本 ADR 记录可复现的本机验证结果，并明确尚未完成的兼容性验证，避免把方案假设当作已验证事实。

基准程序位于 `scripts/knowledge-spikes/`。它是独立 Cargo 工程，不会把模型运行时或测试模型加入桌面应用发布包。

## 已验证结论

### SQLite FTS5

bundled SQLite 已启用 FTS5，且当前 macOS 构建支持 `trigram` tokenizer。

固定样本结果：

| 查询类型  | 查询                  | `unicode61` | `trigram` |
| --------- | --------------------- | ----------: | --------: |
| 中文子串  | `退款审批`            |           0 |         1 |
| 需求编号  | `REQ-1042`            |           1 |         1 |
| 版本号    | `v2.3.1`              |           1 |         1 |
| Rust 方法 | `get_order_detail`    |           1 |         1 |
| Java 类   | `OrderService`        |           2 |         2 |
| API 路径  | `/api/orders`         |           1 |         1 |
| SQL 字段  | `warning_time`        |           1 |         1 |
| 文件路径  | `src/pages/knowledge` |           1 |         1 |

决策：

1. 运行时必须探测 FTS5 和 `trigram`，不能只读取编译选项推断。
2. 支持 `trigram` 时，知识正文默认使用 `trigram`，以满足中文子串检索。
3. 不支持时回退 `unicode61`，需求编号、版本、符号、路径等结构化标识符继续可检索；中文召回由应用层查询拆分和向量通道补充。
4. 项目、版本、来源、敏感级别等字段保持普通表索引和硬过滤，不依赖 FTS tokenizer。

### SQLite 本地向量扫描

向量采用归一化 `f32` little-endian BLOB，维度 384。每次查询先用普通索引把候选限制到 5/20 个项目分区，再顺序扫描约 25% 数据并保留 Top 10。每组预热 2 次，记录后续 10 次。

| 总分块数 | 实际扫描候选 |                 数据库大小 |        构建时间 |        查询 P50 |        查询 P95 |                 最大 RSS |
| -------: | -----------: | -------------------------: | --------------: | --------------: | --------------: | -----------------------: |
|  100,000 |       25,000 | 206,598,144 B（197.0 MiB） |     731～815 ms |   68.4～74.9 ms |   68.9～79.6 ms | 17,776,640 B（17.0 MiB） |
|  200,000 |       50,000 | 413,224,960 B（394.1 MiB） | 1,327～1,399 ms | 134.1～142.1 ms | 145.0～151.0 ms | 18,300,928 B（17.5 MiB） |

决策：

1. 首期继续采用本地 SQLite BLOB + 元数据硬过滤 + Rust 余弦扫描，不部署服务器向量数据库。
2. 查询必须先执行项目、版本、权限、敏感级别和活动 Profile 过滤，禁止无界扫描全部知识。
3. 继续保留设计中的升级阈值：跨项目向量查询 P95 超过 500ms、单机分块持续超过 20 万或数据库/内存压力不可接受时，再评估 sqlite-vec、HNSW 或服务端向量数据库。
4. 当前结果只证明 Apple Silicon macOS 与固定候选比例，不替代 Windows/Linux 和真实数据验收。

### 蓝绿索引生命周期

2026-07-31 执行
`cargo test blue_green_profile_lifecycle_at_100k_chunks --lib -- --ignored --nocapture`，在
本地 SQLite 中创建 100,000 个内部片段并验证以下索引生命周期。测试只写入固定的已归一化
二维向量，因此验收的是数据库完整性和原子状态机，而不是模型语义质量。该测试尚未通过真实
`embedding_build` 批处理服务验证模型错误后的检查点恢复，不能作为 12.5 的完整验收证据。

1. 旧 Profile 覆盖全部片段后才允许激活。
2. 新 Profile 写入 50,000 个向量后，模拟重建任务心跳超时；任务恢复为 `interrupted`，旧
   Profile 保持活动。真实 `embedding_build` 服务从该检查点恢复并完成的验收仍待补充。
3. 不完整的新 Profile 不能完成构建，并会进入失败状态；它不能抢占活动索引。
4. 补齐 100,000 个向量后，新 Profile 经完整性校验后原子激活。
5. 显式回滚至旧 Profile 后，新 Profile 被退休，全部 100,000 个旧向量被清理。

该本机验收耗时约 1.49 秒。它不覆盖真实模型推理、真实构建错误后的恢复、远程 Provider
或跨平台磁盘/内存差异；这些仍由相应 Spike 与外部验收任务负责。

## 已发现的模型运行时约束

`fastembed 5.17.0` 同时声明支持 `MultilingualE5Small`（384 维）和 `BGESmallZHV15`（512 维）。构建期网络验证曾暴露运行时和模型下载风险，因此发布包不允许依赖构建时自动下载。

1. `ort-sys 2.0.0-rc.12` 从官方 CDN 自动下载 ONNX Runtime 1.24.2 时连接被重置。
2. GitHub 官方 ONNX Runtime 1.24.2 arm64 发布包可下载；设置 `ORT_LIB_LOCATION` 与 `ORT_PREFER_DYNAMIC_LINK=1` 后，macOS 编译通过。
3. Hugging Face 主站模型获取被拒绝；镜像可以解析文件，但 `fastembed` 下载 BGE 模型时收到缺少 `Content-Range` 的响应，E5 模型约 448 MiB，镜像下载未在可接受时间内完成。

因此，本地模型实现支持：

- 发布受控的 ONNX Runtime 来源或随应用打包的经过校验的运行时，不能依赖构建期临时联网；
- 模型内部镜像、断点续传、离线导入与 SHA-256 校验；
- 模型文件、运行时、架构、模型 revision 和应用版本兼容矩阵；
- 下载失败时返回结构化错误，禁止自动切换到未授权远程 Embedding。

### 受控 ONNX Runtime 装载（2026-08-01）

本地 Embedding feature 已改为启用 `fastembed/ort-load-dynamic`：构建机不再通过
`ort-sys` 自动下载或静态链接 ONNX Runtime。离线模型包必须在 `runtime/` 子目录中
携带当前平台动态库（Windows `onnxruntime.dll`、macOS `libonnxruntime.dylib`、Linux
`libonnxruntime.so`），并与模型、tokenizer 一起纳入目录 SHA-256 校验。

首次推理前，应用仅从已验证模型包内加载该动态库，并在进程中锁定首个 Runtime 路径；
若后续模型包尝试使用不同 Runtime，必须重启应用，不能静默替换全局 ABI。此变更已完成
macOS feature 编译和边界测试；下文的真实基准进一步确认了模型权重、Tokenizer 与动态库
可共同推理。因此 1.1 已完成；1.2 与 12.7 仍待 Windows/Linux 及完整本地检索链路验收。

### 本地候选模型真实基准（2026-08-01）

在 Apple Silicon macOS 上使用相同的 12 条文档、8 条查询固定集执行真实 ONNX 推理。集合覆盖中文需求和版本号、Rust/Java 标识符、HTTP API、SQL 字段、React 路径、禅道测试事实、英文发布问题及跨项目语义查询。E5 使用 `query:` / `passage:` 前缀和 Mean Pooling；BGE 使用中文检索指令和 CLS Pooling。脚本只从离线包加载模型与 `runtime/libonnxruntime.dylib`，不会下载或回退模型。

| 候选模型 | 权重版本 | 维度 | 初始化 | 20 次向量化 | Recall@1 | MRR | 唯一未命中 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `multilingual-e5-small-int8` | int8 ONNX | 384 | 437 ms | 33 ms | 1.000 | 1.000 | 无 |
| `bge-small-zh-v1.5` | ONNX | 512 | 94 ms | 24 ms | 0.875 | 0.938 | 英文 Jenkins 发布问题命中 Rust Command（目标排第 2） |

离线包完整目录 SHA-256：

- `multilingual-e5-small-int8`：`4d24e2bc01a447951524466ef533e52944bf48509e6552810bcee1a2711cb02c`
- `bge-small-zh-v1.5`：`69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a`
- macOS arm64 ONNX Runtime：`87df6f94dd559ea958748adc80fd4c46d91c52bc025771f513291d155539590a`

决策：首期本地默认 Profile 使用 `multilingual-e5-small-int8`。它在固定混合语料上完整命中且向量维度较低；`bge-small-zh-v1.5` 保留为可选候选，不与 E5 Profile 混用。该选择仅在 macOS arm64 完成真实验证，Windows/Linux 需在相同固定集上复测后方可作为各平台默认值。

### 依赖、解析器与兼容性决策

| 领域 | 选定基线 | macOS arm64 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- | --- | --- |
| 本地 Embedding | `fastembed 5.17.0` + `ort-load-dynamic` | 编译、离线导入、SHA 校验、BGE/E5 推理通过 | 待真实环境验证 | 待真实环境验证 |
| ONNX Runtime | 模型包内受控动态库，首次加载后锁定进程 ABI | `libonnxruntime.dylib` 通过 | 需 `onnxruntime.dll` | 需 `libonnxruntime.so` |
| P0 源码分析 | 内置 `regex` 结构化降级；关系默认未确认 | 五种语言 fixture 通过 | 待交叉编译/运行验证 | 待交叉编译/运行验证 |
| 全文检索 | SQLite FTS5，优先 `trigram`、回退 `unicode61` | 两种 tokenizer 通过 | 待运行时探测 | 待运行时探测 |

当前不引入 Tree-sitter 或专用原生解析器：只有在真实语料精度、干净包体对比及三平台构建均优于结构化降级时才重新评估。性能阈值保持为：活动片段超过约 20 万、跨项目向量检索 P95 超过 500 ms，或前台内存/数据库压力不可接受时，评估可重建的本地 ANN；SQLite 仍为事实来源。

## P0 源码解析器基线（macOS）

当前 P0 分析器是内置的正则结构化降级实现，不含 Tree-sitter、原生解析器或额外
运行时依赖。它覆盖 Rust、TypeScript/JavaScript/TSX、Vue、Java 和 SQL，并且所有
成功结果均显式标记为 `structured_fallback`；因此它可提供确定性符号和关系候选，
但不得表述为 AST 精度。

2026-07-31 在 Apple Silicon macOS 执行
`cargo test services::knowledge_code_analyzer::tests --lib`，3/3 通过：

| 语言                      | Fixture 覆盖                                         | 当前可验证提取      | 质量级别              |
| ------------------------- | ---------------------------------------------------- | ------------------- | --------------------- |
| Rust                      | struct、function、Tauri Command、model、test、config | 符号及框架标记      | `structured_fallback` |
| TypeScript/JavaScript/TSX | interface、function、invoke、route、config、test     | 符号与 IPC/API 候选 | `structured_fallback` |
| Vue                       | `<script setup>`                                     | 组件入口            | `structured_fallback` |
| Java                      | class、Feign、route、test                            | 类与框架标记        | `structured_fallback` |
| SQL                       | `CREATE TABLE`、column                               | 表与列              | `structured_fallback` |

依赖与包体结论：主应用已有的 `regex` 同时被 Tauri 依赖树使用，不能用它单独估算
源码分析器增量包体；当前 release 目标目录包含历史构建产物，亦不能作为精确增量包体。
本机只安装 `aarch64-apple-darwin` Rust target，尚无 Windows/Linux 交叉编译或运行时
证据。因此解析器策略暂定为“结构化降级优先、关系默认未确认”，Tree-sitter/专用解析器
仅在真实语料精度、干净包体对比和三平台构建均通过后再引入。

## 待补验证

- Windows x86_64、Linux x86_64 的编译、运行时加载、下载、离线导入、校验和与真实短文本推理；macOS arm64 已完成离线包加载和两个候选模型的固定检索集；
- 目标禅道实例的版本、认证、分页、限流及字段；
- Rust、TypeScript/JavaScript/TSX、Vue、Java、SQL 解析器的真实样本精度、干净包体和三平台构建；当前仅完成 macOS 结构化降级 fixture 基线。

在这些验证完成前，本 ADR 确认 macOS 的 E5 默认模型、FTS 与 SQLite 向量存储基线；不确认跨平台默认模型和完整依赖矩阵。

## 禅道目标实例只读探测（2026-08-01）

对用户指定的内网测试实例执行了未携带凭据的只读探测；未保存 Cookie、请求 Header 或任何实体正文。实际禅道部署在根站点下的 `/zentao/` 子路径，根路径本身是 XAMPP 欢迎页，不能作为连接根地址。

| 项目 | 脱敏观察 | 结论 |
| --- | --- | --- |
| 产品版本 | 登录页 `window.config.version` 为 `21.7.9` | 目标是现代禅道 21.7.9，使用 `PATH_INFO` 路由 |
| REST 探测 | `/api.php/v1/products?limit=1` 返回 `401` JSON `Unauthorized` | REST v1 候选路径存在且要求认证 |
| 传统模块探测 | `api.php?m=project&f=all` 返回 `PARAM_CODE_MISSING` | 不能将传统模块端点视为无认证或通用回退 |
| 传输安全 | 该内网端口仅提供 HTTP；同端口 HTTPS 握手失败 | 可作为逐连接显式授权的内网 HTTP 例外；默认仍拒绝，Token/Cookie 存在明文传输风险 |

因此 1.5、8.11、8.12 与端到端验收仍未完成。优先方案仍是将该实例置于已校验证书的 HTTPS 反向代理之后。若确认该地址是受控内网，可先在“安全 → 策略”把其主机精确加入 HTTP 域名白名单，再在单个连接中显式启用“允许内网 HTTP”。系统会强制关闭证书校验、二次确认风险，并继续执行安全凭据同源校验、只读 GET、禁止重定向、连接级限流与脱敏审计；每次访问都会重新检查白名单。这不是全局 HTTP 开关，也不降低其他 HTTPS 连接的证书校验要求。随后仍需提供仅具备只读权限的安全凭据引用，才能探测认证后的分页、限流、需求/任务/Bug/测试字段，并采集脱敏真实响应 fixture。

## 复现命令

```bash
cargo run --release --manifest-path scripts/knowledge-spikes/Cargo.toml -- fts
cargo run --release --manifest-path scripts/knowledge-spikes/Cargo.toml -- vector 100000 384
cargo run --release --manifest-path scripts/knowledge-spikes/Cargo.toml -- vector 200000 384
cargo run --release --manifest-path scripts/knowledge-spikes/Cargo.toml -- embedding e5 /absolute/path/to/multilingual-e5-small-int8
cargo run --release --manifest-path scripts/knowledge-spikes/Cargo.toml -- embedding bge /absolute/path/to/bge-small-zh-v1.5
```

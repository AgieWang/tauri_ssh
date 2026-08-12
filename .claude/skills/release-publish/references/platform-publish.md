# 平台发布、产物与 CI 参考

本参考只在正式发布门禁通过后读取。所有 remote、仓库、路径、应用前缀和平台矩阵均从当前项目配置解析；示例占位符不得原样执行。

## 目录

1. 发布拓扑与阶段
2. 平台矩阵和产物契约
3. CI 触发与等待
4. 安全下载和独立目录预检
5. Updater 签名与 manifest
6. Release 仓库写入
7. Android 专项
8. 失败处理

## 1. 发布拓扑与阶段

推荐把源码、CI 草稿 Release、公开更新仓库/CDN 分开：

```text
本地版本准备与已授权 Tag
  -> CI 按 Tag 和平台矩阵构建、签名
  -> CI 全部完成并验证产物
  -> 独立版本目录下载与预检
  -> 生成并校验 manifest
  -> 更新公开 Release 仓库/CDN
  -> 远端回读验收
```

关键顺序：CI 完成并取得完整产物之前，不修改 release 仓库的 README、版本目录、产物或 manifest。避免用户先看到指向不存在文件的新版本。

项目可能有主/备用 CI remote。Tag 只推到用户选定的一个 CI remote，避免重复构建、配额浪费和同 Tag 产物混杂；多个 CI 仓库的 Secrets 独立配置，不能假定共享。

## 2. 平台矩阵和产物契约

从当前 workflow、Tauri bundle targets 和发布配置共同生成“启用 target -> 精确文件集合”。不要硬编码固定产物总数。

当前常见桌面契约如下，但必须以仓库实际 workflow 为准：

| Target | 安装产物 | Updater 产物 | 签名 |
|---|---|---|---|
| Windows x86_64 | NSIS `.exe` | 同 `.exe` | `.exe.sig` |
| macOS aarch64 | `.dmg` | `.app.tar.gz` | `.app.tar.gz.sig` |
| macOS x86_64 | `.dmg` | `.app.tar.gz` | `.app.tar.gz.sig` |
| Linux x86_64 | `.deb`、`.AppImage` | `.AppImage` | `.AppImage.sig` |

若上述四个桌面 target 全启用，常见合计是 11 个文件，而不是 12；平台关闭、workflow 修改或新增架构后必须动态重算。移动 APK/AAB 独立计算，不能混入桌面计数。

每个启用 target 都要逐项验证：

- 必需文件正好一份，名称包含当前 App 前缀、版本、平台/架构；
- 不允许来自其他 App、Tag、旧版本或未启用平台的多余文件；
- 需要 Updater 的 target 正好有一个对应 `.sig`；
- 文件非空、大小合理，必要时计算并记录 hash；
- 所有 target 都通过后才进入 manifest 和外部发布。

## 3. CI 触发与等待

1. 推 Tag 前确认目标 remote、Tag 不冲突、目标提交 SHA 和 CI workflow 分流。
2. 推送后通过 Tauri SSH MCP 或受控 GitHub 能力读取运行状态；凭据不得进入 Shell 文本或模型上下文。
3. 轮询必须有合理间隔和总超时；API/JSON 解析失败按失败处理，不能把空值解释为仍在运行。
4. 只接受与目标 Tag/SHA 对应且所有必需 job 成功的 run；跳过、取消、排队或部分平台成功不算完成。
5. CI 失败或超时立即停止，不创建公开版本目录或 manifest。

如果不得不用 HTTP 下载接口，请使用失败即非零、连接/总超时、状态码检查和重试上限。下载后核对响应内容类型、文件大小及可用的 hash；不能只用 `curl -s` 后假定成功，也不能把错误 JSON 当二进制产物。

## 4. 安全下载和独立目录预检

每个 App、版本、Tag 使用新的独立下载目录。目录必须是已解析的具体子目录，不能用共享 Downloads 根目录、宽 glob 或未校验变量。

下载资产前先从 Release API 取得结构化清单，再按以下条件精确筛选：

- Tag 与目标 Tag 完全一致；
- 文件名以当前 App 产物前缀开头；
- 名称与启用 target 的预期模式完全匹配；
- 每个预期资产 ID 唯一。

下载后重新扫描独立目录并与预期集合做集合相等比较。缺失、多余、重复、零字节、HTTP 错误页或 hash 不一致都失败。不要用 `cp source/*.sig`、静默 `2>/dev/null` 或“存在就复制”的流程掩盖缺失。

复制到两个 release 仓库时，逐个使用预检清单中的绝对源文件和明确目标文件；复制后比较文件数、大小与 hash。目标版本目录如果已存在，先停止并确认是续传、修复还是版本冲突，不能覆盖。

## 5. Updater 签名与 manifest

签名读取和注入遵循以下约束：

1. 对每个启用 updater target，用 App 前缀 + 版本 + target 精确匹配 `.sig`，结果必须正好一个。
2. 去除换行后验证非空、长度边界和 Base64 字符；执行真实 Base64 解码，并重新编码比对规范化值。
3. 禁止 `eval`、宽 glob、拼接多个 `.sig` 或人工复制签名字符串。
4. 用结构化 JSON 库生成 manifest，不用字符串替换或 Shell 拼 JSON。
5. manifest 只包含启用 target；每个 target 的 URL 指向对应 updater 产物，signature 来自对应 `.sig`。
6. 生成后重新解析 JSON，逐 target 比对版本、URL、signature 与原始预检清单；所有启用 target 都必须一致。
7. 对公开 URL 做失败即非零、状态码、内容长度和 hash/大小校验；至少执行一次真实 updater 检查。

桌面安装产物和 Updater 产物可能不同，例如 macOS `.dmg` 用于人工安装、`.app.tar.gz` 用于 updater。不得把安装下载 URL 填入 updater target。

## 6. Release 仓库写入

1. 对每个本地 release 仓库先执行只读状态检查，确认没有其他会话未提交改动。
2. 在修改前从目标远端 pull/rebase 或使用仓库既有安全同步策略；冲突时停止，禁止自动覆盖。
3. 更新 README、版本目录、产物和 manifest 后，逐文件暂存；禁止 `git add -A` / `git add .`。
4. 提交前核对 staged 文件、版本目录和 diff，再按用户授权 push。
5. 任一仓库 push 失败立即停止后续发布步骤，不把失败交给用户后继续宣称完成。
6. push 成功后回读远端提交 SHA、文件列表、manifest 和下载 URL；两个远端分别验证。

CDN/R2 属于额外外部目标，只有用户明确授权且配置存在时执行。上传同样逐文件、校验大小/hash、回读公开 URL；不能因为 release 仓发布被授权就自动扩大到 CDN。

## 7. Android 专项

Android 与桌面发布使用独立版本和 Tag 流：桌面通常为 `vX.Y.Z`，移动端为 `mobile-vX.Y.Z`。CI job 必须按前缀互斥，产物目录和更新通道也隔离，避免桌面/移动在同一 Tag 或目录混合。

发布移动端前必须核对：

- 项目定义的四处移动版本保持一致；具体文件从当前移动端 Skill/仓库读取，不硬编码旧路径；
- `versionCode` 严格大于所有已发布/已安装版本，即使 `versionName` 回退也不能回退；
- 使用长期稳定的 release keystore，keystore 与密码从受控 Secrets 注入，绝不提交或输出；
- 所有 CI 仓库都配置同一稳定签名材料，不能静默回退到临时 debug keystore；
- 对每个 APK 用 `apksigner` 验证证书指纹与预期 release keystore 一致，再允许上传；
- APK/AAB 资产按 App 前缀和移动 Tag 唯一匹配，不能生成桌面 Updater manifest；侧载发布走移动端自己的版本清单/下载通道。

稳定 keystore 丢失通常无法为既有安装用户发布可覆盖升级，必须异地安全备份。重新生成 keystore 是新的破坏性发布决策，不得自动执行。

## 8. 失败处理

| 失败 | 处理 |
|---|---|
| CI run 找不到/解析失败 | 停止；核对 Tag、remote、权限与 API，不无限轮询 |
| 产物缺失或多余 | 停止；从目标 CI/Tag 重新取得独立目录，不混用旧文件 |
| `.sig` 非唯一或 Base64 不可往返 | 停止；定位正确 App/target 签名，不人工修剪或粘贴 |
| manifest 与原始签名/URL 不一致 | 删除本次未发布的生成物并结构化重建；不上传 |
| release 仓冲突 | 停止并人工审查冲突，不强推、不 reset |
| push/上传失败 | 停止后续外部写入，记录已完成远端对象并报告恢复方案 |
| Android 指纹不一致 | 拒绝发布；核查 Secrets 与 CI 签名步骤，禁止发布 debug 签名 |

任何删除、覆盖、重打同名 Tag、替换已发布 manifest 或回退公开版本都需要新的明确授权。优先发布修复版本，避免改写已被客户端消费的发布对象。


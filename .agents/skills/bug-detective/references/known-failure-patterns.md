# 项目已知故障模式

这些模式来自项目历史经验，仅用于快速提出假设。版本、依赖和实现可能已变化；不能看到相似症状就直接判定根因或修改代码。

## Rust 与文件

| 症状 | 历史根因 | 当前验证方向 |
|---|---|---|
| 高频生成文件时偶发覆盖 | Windows 时钟分辨率下，timestamp + nanos 仍可能碰撞 | 检查命名实现和并发复现；需要唯一名时在时间戳后加入进程内 `AtomicU64` 序号 |

## Tauri WebView 与 Ant Design

| 症状 | 历史根因 | 当前验证方向 |
|---|---|---|
| HTML5 页内拖拽显示禁止光标，`onDrop` 不触发 | Tauri 窗口原生文件拖放捕获了 WebView 事件 | 核对 `dragDropEnabled` 和目标功能；若应用依赖页内拖拽，评估设为 `false` 后的原生文件拖入影响，并重启验证 |
| Ant Design Tree 节点被右键 Dropdown 包裹后不能拖动 | rc-trigger 的 ref/鼠标事件破坏 Tree 原生拖拽绑定 | 用浏览器检查事件目标；可验证 Tree 级 `onRightClick` + 独立定位菜单方案 |
| Modal 编辑/克隆打开时表单为空，新建正常 | `destroyOnClose` 且 Modal 关闭状态调用 `setFieldsValue`，Form.Item 尚未挂载 | 验证表单生命周期；可用受控 `key` + `initialValues` 在打开时重建，而非依赖关闭态赋值 |
| `@tanstack/react-virtual` 完全没有可见行 | 滚动容器只有 `maxHeight`，叠加 `contain: strict`/`contain: size` 后测得高度为 0 | 在浏览器读取容器实际尺寸；改为明确高度或不包含 size 的 containment 后复验 |
| `Sider` 设置 flex 后子元素仍纵向堆叠 | 真正承载 children 的 `.ant-layout-sider-children` 仍是 block | 检查 DOM；在 Sider 内增加占满高度的自有 flex 容器，避免依赖外层 aside 的布局传播 |

以上页面问题必须使用 Codex 内置浏览器或 Control Chrome 复现和验收。

## 本地服务与脚本

| 症状 | 历史根因 | 当前验证方向 |
|---|---|---|
| `localhost` 请求返回 502，但监听进程存在 | `http_proxy/https_proxy` 把回环请求发给代理 | 检查代理环境；用 `--noproxy '*'` 或正确的 `NO_PROXY=localhost,127.0.0.1` 做只读复验 |
| 自动化请求固定端口失败，Vite 日志显示改用其他端口 | 已有进程占用预期端口，Vite 自动选择新端口 | 读取启动日志和监听信息，使用实际 URL 或复用现有服务；禁止擅自 kill 端口/进程，确需终止时先请求协调 |
| Windows Node 脚本读取 `/tmp/...` 报 `ENOENT` | Unix 临时目录硬编码在 Windows 被解析为盘符根目录 | 检查路径来源，改用 `os.tmpdir()` 或项目内受控临时目录 |
| Windows 下子进程环境变量语法无效 | Git Bash、CMD、PowerShell 语法混用 | 识别实际 shell；优先通过子进程 `env` 参数传值，或使用对应 shell 的正确语法 |

## 移动端构建与更新

| 症状 | 历史根因 | 当前验证方向 |
|---|---|---|
| Android 新 APK 无法覆盖安装旧版 | CI 未使用稳定 release keystore，构建之间签名指纹变化 | 检查签名配置和 `apksigner verify --print-certs` 指纹；凭据通过安全配置注入，不输出 keystore 密码 |
| 移动端检查更新始终判断为最新版 | semver 解析未剥离 `mobile-`/`mobile-v` 前缀 | 用真实版本字符串做单元测试，规范化前缀后再比较 |
| 中文路径下 Android 链接失败 | NDK/链接器工具链无法稳定处理路径 | 核对实际错误；将 Cargo target 目录配置到受控 ASCII 路径，不移动或清理他人工作区 |
| Mobile 子项目 Rollup 无法解析 re-export 中的 `@/` | 子项目构建链未应用根项目别名 | 追踪 re-export 链；在移动端共享出口使用明确相对路径并运行目标构建 |

## 使用规则

1. 先确认依赖版本、配置和当前源码仍符合历史前提。
2. 为候选根因寻找直接证据和反证。
3. 修复前建立最小复现，修复后运行对应测试和真实 UI/目标平台验证。
4. 端口、签名、代理和凭据相关检查不得破坏其他会话或泄露敏感信息。

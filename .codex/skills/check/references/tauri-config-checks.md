# Tauri 配置与权限检查

## 配置一致性

校验 JSON/TOML 语法，并核对 `package.json`、Cargo、`tauri.conf.json`、Vite 和发布配置的版本、产品名、identifier、devUrl、端口、bundle 与 updater 值。

identifier 使用合法反向域名并保持稳定；变更 identifier 会影响应用数据目录、更新和安装身份，必须作为高风险兼容性变更审查。

## 插件注册链

每个 `@tauri-apps/plugin-*` 或 Rust 插件应核对：

1. npm 与 Cargo 依赖版本兼容。
2. `tauri::Builder.plugin(...)` 已注册。
3. 前端 API 使用与插件版本一致。
4. 对应 Capability permission 已声明。
5. 平台条件编译和移动端差异已处理。

不要维护静态“插件到权限”真理表来替代当前生成 schema 和官方权限标识；读取项目当前配置与依赖确认。

## Capabilities

- 权限遵循最小化，scope 限定窗口、路径、URL 或命令范围。
- 代码使用的能力必须有声明，未使用的高权限项应报告。
- `shell`、`fs`、`http`、process、opener 等高风险能力审查参数来源和注入面。
- 多窗口应用确认 `windows`/`webviews` 作用域；不能让非目标窗口继承敏感权限。
- Capabilities 变更需要真实运行时调用验证，JSON 解析通过不等于权限生效。

## CSP 与外部资源

- 生产 CSP 尽可能收紧；`unsafe-eval`、宽泛 `connect-src`、任意外部 URL 需要证据和风险说明。
- 外部 URL、deep link、opener 和 HTTP scope 使用 allowlist，并验证重定向与用户输入。
- 开发环境例外不能无条件进入生产配置。

## 建议验证

```bash
node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8'))"
(cd src-tauri && cargo check)
pnpm build
git diff --check
```

随后在 Tauri 应用中执行实际插件能力，检查允许路径成功、越界路径被拒绝、错误信息可理解且不泄露敏感信息。

## 审查清单

- [ ] JSON/TOML、identifier、端口、版本和 bundle 一致。
- [ ] 插件依赖、注册、前端调用和 Capability 全链路完整。
- [ ] 权限与 scope 最小化，高风险输入有边界。
- [ ] CSP 和外部 URL 没有无依据放宽。
- [ ] 真实运行时允许/拒绝行为已验证。

---
name: tauri-plugins
description: |
  用于引入、配置、升级或开发 `tauri-plugin-*` 及其前端绑定；普通 npm 包、crate 或第三方业务 SDK 不触发。

  触发场景：
  - 添加或升级 Tauri 官方/社区 `tauri-plugin-*`
  - 在 `tauri::Builder` 注册插件并配置初始化参数
  - 配置插件前端包、Capabilities 和跨平台支持
  - 开发自定义 Tauri 插件或诊断插件未注册/权限缺失

  触发词：tauri-plugin、@tauri-apps/plugin、Builder.plugin、插件注册、Tauri plugin init、自定义 Tauri 插件、插件 permission、插件前端绑定
---

# Tauri 插件集成

## 边界

本技能只处理 Tauri 插件生命周期。普通 npm/crate 依赖、React 组件库、Rust 业务模块和第三方 HTTP SDK 不应触发。具体 permission/scope 使用 `tauri-capabilities`；插件涉及凭据、Shell、文件、外部 URL 或远程访问时同时使用 `security-permissions`。

## 强制规则

1. 读取当前 `Cargo.toml`、`package.json`、`src-tauri/src/lib.rs` 和 Capabilities，确认项目已有插件与版本。
2. Rust crate、前端绑定和 Tauri 主版本必须兼容；不得只安装一侧依赖。
3. 在 `tauri::Builder` 明确注册插件，并把业务逻辑放在 Service/API 封装层，避免页面裸调或 Command 堆积逻辑。
4. 权限按实际 API 最小声明；不能因插件不可用直接开放 `*:default` 或宽 scope。
5. 检查 Windows/macOS/Linux 支持差异、移动端条件编译和功能降级。
6. 新依赖要评估维护状态、许可证、供应链和安全边界，不把“能编译”当成集成完成。

## 执行流程

1. 明确所需插件能力和为什么不能由现有模块完成。
2. 对照当前锁文件与官方版本安装 Rust/TypeScript 依赖。
3. 注册插件、配置初始化、补最小 Capability，并通过 `src/lib/api/` 封装前端调用。
4. 验证编译、权限允许/拒绝路径、平台行为和卸载/失败降级。
5. 变更 lock 文件时核对依赖树与不必要升级。

## 按需参考

需要官方插件清单、初始化方式、自定义插件结构、Service/前端调用示例或故障排查时读取 [references/plugin-integration.md](references/plugin-integration.md)。

只有实际要改 permission/scope 时再加载 `tauri-capabilities`；只有涉及自动更新协议时再加载 `tauri-updater`。

## 完成条件

- Rust/前端依赖、注册、权限和 API 封装形成完整链路。
- 最小权限与目标平台行为已验证，无敏感配置泄露。
- Cargo/TypeScript 检查、聚焦测试和构建通过。
- UTF-8 无 BOM，`git diff --check` 通过。

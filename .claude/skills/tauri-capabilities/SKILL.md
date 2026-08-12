---
name: tauri-capabilities
description: |
  用于修改或诊断 Tauri 2 的 `capabilities/*.json`、permission、scope 与窗口权限映射；仅在存在具体 Capability 配置证据时触发。

  触发场景：
  - 修改 `src-tauri/capabilities/*.json` 的 permissions 或 windows/webviews
  - 为 `tauri-plugin-*` 添加、收紧或排查 permission/scope
  - 配置文件路径、URL、Shell 或多窗口的细粒度作用域
  - 根据生成 schema 诊断运行时 permission denied

  触发词：capabilities/default.json、Tauri Capability、permission identifier、scope、窗口权限、webview 权限、permission denied、desktop-schema
---

# Tauri 2 Capabilities

## 边界

本技能只负责 Capability JSON 与运行时权限映射。威胁边界、凭据、CSP 和外部输入审查使用 `security-permissions`；插件依赖和 `.plugin(...)` 注册使用 `tauri-plugins`。

普通业务角色权限、页面按钮权限、操作系统文件权限，以及没有 Tauri Capability 证据的“没权限”问题不应触发。

## 强制规则

1. 先读取当前 `src-tauri/capabilities/`、`tauri.conf.json`、插件注册和生成的 permission schema，不凭记忆拼权限标识。
2. 每项 permission 必须对应真实使用的 API/Command/插件；新增权限必须说明调用方、窗口与资源范围。
3. 优先专用 permission 和最窄 scope；禁止无理由使用所有窗口、全盘路径、任意 URL 或 Shell 通配。
4. 多窗口按 label/角色拆分 Capability，避免让低信任窗口继承主窗口全部能力。
5. 修改插件权限时，同时核对 Cargo/npm 依赖、Rust 注册和前端 API，不能只改 JSON。
6. 涉及凭据、外部 URL、Shell、远程或敏感文件时，必须同时应用 `security-permissions`。

## 执行流程

1. 定位失败或需求对应的 Tauri API、插件和调用窗口。
2. 从当前 schema 确认 permission 名、允许的 scope 结构与平台限制。
3. 在现有 Capability 上做最小增量；需要隔离时新增按窗口划分的 Capability。
4. 校验 JSON、identifier、windows/webviews、permission 和 scope 路径。
5. 在真实 Tauri 运行时分别验证允许路径与拒绝路径；仅通过前端构建不算完成。

## 按需参考

修改 permission/scope、多 Capability 或多窗口配置前，读取 [references/permission-and-scope.md](references/permission-and-scope.md)。该文件包含详细 JSON、路径变量、窗口匹配、schema 查看和排错示例。

## 完成条件

- 权限标识来自当前 schema，且与插件注册及调用代码一致。
- scope 和窗口范围已最小化，并保留预期拒绝行为。
- JSON 校验、相关测试/构建和真实运行时权限验证通过。
- UTF-8 无 BOM，`git diff --check` 通过。

---
name: doc-generation
description: |
  用于根据当前项目代码生成项目内开发者 Markdown、Command/API 参考、IPC 映射或 SQLite Schema 说明；不管理 VitePress 站点。

  触发场景：
  - 用户明确要求生成项目内 Command/API 开发者参考
  - 用户明确要求生成模块调用链或 IPC 映射文档
  - 用户明确要求生成 SQLite Schema 或迁移开发文档

  触发词：开发者文档、Command接口参考、IPC映射文档、SQLite Schema文档、模块开发文档、doc-generation
---

# 项目内开发者文档生成

## 职责边界

本技能只生成项目仓库 `docs/` 下供开发者阅读的 Markdown，例如：

| 文档类型 | 默认位置 | 主要证据 |
|---|---|---|
| Command/API 参考 | `docs/commands/` | Rust Command、注册表、前端 API 封装 |
| 模块开发文档 | `docs/modules/` | Command → Service → Database、React 页面 |
| 数据库开发文档 | `docs/database/` | Schema 迁移、DAO、Model、真实 DDL/数据格式 |
| IPC/事件映射 | `docs/commands/` 或用户指定的 `docs/` 子目录 | `invoke`、Command、`emit/listen` |

以下情况不触发：

- VitePress 对外站点、用户手册、站点初始化或 `.docs-meta.json` 同步，改用 `docs-management`。
- 普通内部方案、决策记录、需求说明或手工撰写的一篇 Markdown。
- 用户未明确要求生成文档。项目规则禁止主动创建文档。

## 强制规则

1. 只有用户明确要求时才创建或更新文档，且文档必须写到项目根 `docs/` 内。
2. 先读真实源码和当前注册关系，再写结论；不得按目录名、样例或过期文档猜测实现。
3. Command 文档必须同时核对 `#[tauri::command]`、`generate_handler!` 注册、TypeScript 类型、`src/lib/api/` 封装及真实调用点。
4. 数据库文档以 Schema/迁移和 DAO 为基础；涉及真实 DDL 或数据格式时，读取配置并通过 Tauri SSH MCP 查询，不能只看示例。
5. 明确区分“代码中已实现”“运行时已验证”“尚未确认”，不把扫描结果写成生产事实。
6. 保留用户手工内容。除非用户明确要求重写，不覆盖整篇现有文档；优先更新可识别的生成段落。
7. 源码、配置和 Markdown 均使用 UTF-8 无 BOM；中文必须可读。
8. 文档中的示例不得包含真实凭据、生产数据、绝对用户路径或无法验证的返回值。

## 生成流程

1. 明确文档受众、类型、模块范围和目标 `docs/` 子目录。
2. 读取对应代码链路；必要时核对配置、数据库和运行时证据。
3. 生成“来源清单”，确保每个接口、字段、权限和错误说明都有证据。
4. 按目标类型生成或增量更新 Markdown；标注未确认项。
5. 复核路径、链接、类型名、字段名、Command 注册和前后端命名转换。
6. 对修改后的文档执行 UTF-8/BOM、乱码、链接和 `git diff --check` 检查。

## 按需读取

- Command、模块、数据库、IPC 的扫描范围与字段清单：
  [source-scanning.md](references/source-scanning.md)
- 文档结构和输出模板：
  [developer-doc-templates.md](references/developer-doc-templates.md)

只读取当前文档类型对应的章节，不默认加载全部模板。

## 完成条件

- 输出位于项目 `docs/` 内，范围与用户请求一致。
- 所有字段、接口和调用关系可追溯到当前代码或明确标注的运行时证据。
- 没有覆盖无关的手工文档内容，没有写入敏感信息。
- UTF-8 无 BOM、中文可读，链接和差异检查通过。

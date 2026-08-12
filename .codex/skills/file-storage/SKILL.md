---
name: file-storage
description: |
  用于实现 Tauri 应用内的本地文件系统能力，包括安全读写、导入导出、拖放和文件对话框。

  触发场景：
  - 通过 Rust std::fs 或 tauri-plugin-fs 读写用户文件
  - 实现文件/目录选择、保存对话框或拖放导入
  - 解析 app_data_dir 等跨平台应用目录
  - 设计文件导入导出、原子写入、路径范围或 Capabilities

  不应触发：编辑仓库源码/文档、查找代码文件、读取 SKILL.md、普通 Git 文件变更。

  触发词：std::fs、tauri-plugin-fs、tauri-plugin-dialog、app_data_dir、文件导入、文件导出、文件拖放、原子写入
---

# Tauri 文件系统能力

## 适用边界

本 Skill 只处理“应用功能要访问本地文件系统”。Codex 使用工具编辑仓库文件、定位文件或生成文档，不属于此 Skill。仅出现“文件、保存、打开、目录”不得自动触发。

## 方案选择

| 场景 | 首选 |
|---|---|
| Rust 业务逻辑、批量处理、受控路径 | Rust Command + `std::fs`/异步文件 API |
| 简单前端文件读写且 scope 清晰 | `tauri-plugin-fs` |
| 用户主动选择输入/输出位置 | `tauri-plugin-dialog` |
| 应用内部数据 | Tauri path API 的 app data/config 目录 |

Command 仍遵守 Command → Service → 文件访问逻辑分层；前端调用封装到 `src/lib/api/`。

## 安全与正确性规则

1. 路径来自用户或外部输入时，解析规范化路径并验证其位于允许范围；防止 `..`、符号链接和路径穿越绕过。
2. 不硬编码 `C:\\Users`、`/Users/...` 或 home；使用 Tauri path API 和 `PathBuf`/`join`。
3. 文件插件必须在 `capabilities/*.json` 使用最小权限和最小 scope；不要为方便开放整个主目录。
4. 重要配置或可恢复数据使用“同目录临时文件 → flush/sync（按风险）→ 原子替换”，避免半写入。
5. 覆盖、删除、批量移动等破坏性操作先解析精确目标并获得请求范围内授权；优先可恢复方案。
6. 文件内容视为不可信输入：限制大小、校验扩展名不能替代内容校验、解析失败必须可诊断。
7. 禁止把凭据或敏感数据写入普通文本、日志和前端可读 store。

Rust、FS plugin、Dialog、拖放和路径示例见 [filesystem-patterns.md](references/filesystem-patterns.md)。

## 实施流程

1. 明确用户选择路径还是应用受控路径、读/写/覆盖语义、最大文件和支持格式。
2. 读取现有 Command、Capabilities 和同类文件功能，复用错误模型与 API 封装。
3. 选择后端或插件方案，配置最小权限；对输入做路径和内容校验。
4. 覆盖不存在、无权限、取消对话框、超大文件、乱码、部分写入和跨平台路径测试。
5. 页面交互变更使用内置浏览器或 Chrome 验证选择、取消、成功和失败反馈。

## 不应触发示例

- “修改 `src/App.tsx`”或“读取一份 Markdown”。
- “查找数据库实现在哪个文件”——使用项目导航。
- “把生成的方案保存到 docs/”——这是仓库文档操作，不是应用文件功能。

## 完成条件

- 路径范围、Capabilities、覆盖/删除语义和错误处理明确。
- 跨平台路径与对话框取消行为正确，输入大小和格式经过校验。
- 相关 Rust/TypeScript 测试、格式化、类型检查通过；页面流程已浏览器验收。
- UTF-8 无 BOM，`git diff --check` 通过。

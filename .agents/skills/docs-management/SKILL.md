---
name: docs-management
description: |
  用于初始化、更新和校验面向用户的 VitePress 文档站点；不处理项目 `docs/` 下的普通内部方案或开发者 Markdown。

  触发场景：
  - 用户明确要求初始化 VitePress 对外文档站点
  - 用户明确要求按代码变更增量同步现有 VitePress 站点
  - 用户明确要求检查 `.docs-meta.json` 或 VitePress 站点同步状态

  触发词：VitePress文档站点、对外文档站点、docs-management、.docs-meta.json、website文档站点、文档站点同步
---

# VitePress 文档站点管理

## 职责边界

本技能只管理产品级、面向用户的 VitePress 站点，包括初始化、增量同步、全量重建和同步元数据。

以下情况不触发：

- 项目 `docs/` 下的内部方案、ADR、任务说明或普通 Markdown。
- 根据代码生成开发者 Command/API、模块、数据库参考，改用 `doc-generation`。
- 仅出现“写文档”“更新文档”等宽泛表达，但没有 VitePress 或站点意图。

站点默认放在同级独立仓库 `../{project}-docs`；若用户明确要求，也可放在本项目 `./website/` 或用户指定目录。创建同级仓库或修改外部目录前必须确认目标范围。

## 不可下沉的保护规则

1. `.docs-meta.json` 放在主项目根目录，用于记录 `lastSyncCommit`、文档位置和同步历史。
2. 不占用主项目 `./update-docs/`；该目录属于内部研发文档。
3. VitePress 依赖与命令只存在于文档目录，不污染主项目根 `package.json`。
4. 增量更新只修改明确标记的自动生成段落；保留手工段落、用户新增 Markdown 和 `docs/public/` 资源。
5. 同模块变更聚合到同一章节，避免“一份源文件一篇文档”。
6. 禁止自动 `git push`。Git 初始化、暂存、提交或远端操作必须符合当前用户授权和多会话并发协议；只暂存本次文档范围内文件。
7. 禁止在模板、元数据、示例或站点页面中写入真实凭据、内网地址、生产数据和本机绝对路径。
8. 写入前展示目标路径和影响章节；目标不明确时不得初始化或全量重建。
9. 全量重建不删除手工文件与图片，不把未确认的代码扫描结果描述为已上线能力。
10. 所有生成文件使用 UTF-8 无 BOM；中文内容必须可读。

## 决策流程

```text
用户明确提出 VitePress 站点需求
├─ 无 .docs-meta.json → 初始化站点
│  ├─ 确认 sibling / ./website / 自定义路径
│  ├─ 读取项目名、描述、技术栈和当前版本
│  └─ 从模板生成并写入初始元数据
├─ 有 .docs-meta.json → 增量同步
│  ├─ 校验 docsPath 与 lastSyncCommit
│  ├─ 根据 git diff 聚合受影响章节
│  └─ 用户确认后更新生成段落与同步历史
└─ 用户明确要求 full → 全量重建
   └─ 保留手工文档、图片和非生成段落
```

## 执行入口

1. 确认任务确实是 VitePress 站点，并读取 `.docs-meta.json`（如存在）。
2. 只读检查目标路径、模板、主项目配置、Git 状态和受影响源文件。
3. 展示将创建或更新的站点位置、章节和元数据，再进行写入。
4. 初始化时按参考文件替换占位符；增量同步时按路径映射聚合变化。
5. 检查站内链接、VitePress 配置、构建和页面呈现；页面验证使用 Codex 内置浏览器或 Control Chrome。
6. 更新元数据时保留历史，只追加准确的本次记录。

## 按需读取

- 首次建站、目标选择和占位符：
  [site-initialization.md](references/site-initialization.md)
- 增量同步、路径映射和 `.docs-meta.json`：
  [incremental-sync.md](references/incremental-sync.md)
- 站点写作风格、构建和浏览器验收：
  [site-content-and-verification.md](references/site-content-and-verification.md)

只读取当前流程对应的参考文件。内部 Markdown 请求不读取任何 VitePress 初始化参考。

## 完成条件

- VitePress 目标和写入范围经过确认，没有侵入主项目内部文档目录。
- 手工内容、用户资源和历史元数据得到保留。
- 未自动推送，未越界暂存或提交其他会话文件。
- VitePress 配置与构建通过，真实浏览器页面完成验证。
- 文档内容有源码证据，UTF-8 无 BOM且无敏感信息。

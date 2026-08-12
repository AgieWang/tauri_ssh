---
name: update-docs
description: 用于用户显式调用 /update-docs 的 init、update、full、status、diff 子命令管理 VitePress 对外站点；内部 docs 方案、README 或普通“更新文档”不触发。
---

# /update-docs

这是显式 VitePress 站点工作流。普通内部 `docs/` 方案文档、README 编辑、代码注释和“更新文档”不触发。

具体状态机按需读取 [`references/site-workflows.md`](references/site-workflows.md)，VitePress 结构和映射规则由 `docs-management` 提供；本入口不复制完整站点教程。

## 子命令

| 输入 | 行为 | 写入 |
|---|---|---|
| `/update-docs status` | 读取站点位置、同步基线和变更数 | 否 |
| `/update-docs diff` | 预览源文件到文档章节的影响 | 否 |
| `/update-docs init` | 初始化站点和元数据 | 确认后 |
| `/update-docs update` | 从同步基线增量更新受影响章节 | 影响表确认后 |
| `/update-docs full` | 重建受管章节，保留人工资产 | 影响表确认后 |
| `/update-docs` | 有元数据时等同 `update`；无元数据时只进入初始化信息收集 | 仍需确认 |

## 编排步骤

1. 读取主项目根 `.docs-meta.json`、Git 状态和目标 docsPath；不存在时不得猜测目标路径。
2. 加载 `docs-management`，按元数据和真实源码建立影响表。
3. 展示目标目录、将写文件、将保留文件、同步基线和回滚方式。
4. `status`/`diff` 直接只读输出；`init`/`update`/`full` 必须在用户确认影响表后写入。
5. 只更新受管标记段或受管章节；保留人工段落、非模板 Markdown、图片和 public 资产。
6. 验证站点构建、链接、UTF-8、元数据一致性和 `git diff --check`；页面验收按内置浏览器 → Chrome 顺序执行。

## 写入与 Git 边界

- `.docs-meta.json` 位于主项目根，站点在内部时使用 `website/`，不占用研发文档目录。
- 跨仓库/sibling docsPath 是独立写入目标，必须确认绝对路径和 Git 状态，不能覆盖其未提交改动。
- 默认不 commit、不 push。用户明确要求提交时逐文件暂存；push 必须另有明确授权。
- 全量重建不是删除授权；不得删除人工文件，无法判断的内容列为待确认。
- 失败时仅还原本轮生成的受管段和元数据变化，不用 reset、checkout、clean 或 stash。

## 不应触发

- “写一份 Skill 优化方案到 docs”——普通内部文档。
- “更新 README 中的一行”——直接编辑目标文件。
- “同步项目状态和待办”——使用显式 `/sync` 或 `/update-status`。

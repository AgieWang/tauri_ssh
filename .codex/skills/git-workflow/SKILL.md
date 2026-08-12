---
name: git-workflow
description: |
  用于 Git 分支、逐文件暂存、提交、合并、拉取和推送操作；安装包、签名、Tag 发布与 Release 产物交给发布技能。

  触发场景：
  - 创建或管理 Git 分支、提交、合并与上游关系
  - 规范 Conventional Commits 或核对提交范围
  - 执行 pull/fetch/push 等远程 Git 操作
  - 在多会话脏工作区中安全暂存和提交本任务文件

  触发词：git status、创建分支、逐文件暂存、git commit、git merge、git pull、git push、Conventional Commits、上游分支
---

# Git 工作流

## 边界

本技能只处理 Git 对象和工作区操作。构建安装包使用 `tauri-packaging`；签名、Release、发布 Tag、远端产物和 update.json 使用 `release-publish`/`tauri-updater`。普通代码修改不因最终可能提交而自动加载本技能。

## 多会话安全规则

1. 会话首次执行 `git status -s` 和 `git branch --show-current`，记录未提交清单与分支，不复述无关业务改动。
2. 修改已有文件前用 `git log -1 --format="%ar|%s" -- <file>` 判断近期占用；相关未提交且 15 分钟内必须改同文件时才询问用户。
3. 不修改、不回滚、不格式化、不暂存与当前任务无关的文件。
4. 禁止 `git stash`、`git reset --hard`、`git clean -fd`、通过 checkout/restore 丢弃改动以及未经用户要求切换分支。
5. 禁止 `git add .` 和 `git add -A`；逐个 `git add <具体文件>`。
6. 提交前读取 `git diff --cached --name-only`，静默取消暂存越界文件，再核对 staged diff。

## 远程与授权

Git、服务器和凭据操作优先使用 Tauri SSH MCP/Safe Credentials。fetch 等只读操作可按任务需要执行；push、强推、删除远端分支和改写历史属于外部写入，必须在用户请求范围内。不得输出含凭据的 remote URL。

发布 Tag 即使是 Git 操作，也属于正式发布终端动作，交给 `release-publish`；本技能不得仅因“版本发布”自行打 Tag 或推送。

## 执行流程

1. 读取状态、分支、上游、远端和目标文件 diff，确定本次 Git 范围。
2. 选择与仓库一致的分支/提交策略；不为普通任务擅自切分支或建 worktree。
3. 运行变更对应的格式化、检查和测试。
4. 逐文件暂存，核对 staged 文件与 staged diff，生成准确提交信息。
5. 仅在用户请求包含远程写入时 push，并读取远端状态验证结果。
6. 报告提交 SHA、分支、远端结果和未包含的工作区改动，不宣称未验证状态。

## 按需参考

需要分支命名、Conventional Commits、scope 示例或基础 Git 命令时读取 [references/git-operations.md](references/git-operations.md)。其中历史发布段落仅用于识别边界，实际发布必须转交 `release-publish`。

## 完成条件

- staged/committed 文件严格属于本任务，其他会话改动保持原状。
- 提交信息与 diff 一致，适用检查通过。
- 所有远程写入均有授权并通过远端读取验证。
- UTF-8 无 BOM，`git diff --check` 通过。

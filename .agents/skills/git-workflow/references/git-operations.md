# Git 分支、提交与远程操作参考

## 目录

1. 状态与范围
2. 分支命名
3. 提交信息
4. 暂存与提交
5. 远程操作
6. 冲突与禁止项

## 1. 状态与范围

开始前读取：

```bash
git status -s
git branch --show-current
git remote -v
```

输出 remote 时确认 URL 不包含用户名、Token 或密码。对已有文件修改前按项目多会话协议查询最近提交；未提交清单中与任务无关的文件属于他者占用区。

## 2. 分支命名

优先遵循仓库现有约定。没有明确约定且用户要求建分支时，可使用：

| 用途 | 示例 |
|---|---|
| 功能 | `codex/feature-name` |
| 修复 | `codex/fix-issue` |
| 重构 | `codex/refactor-module` |

不要因为任务复杂就自动建分支或 worktree；用户明确并行开发或隔离风险确有必要时再执行。

## 3. 提交信息

使用仓库既有格式；采用 Conventional Commits 时：

```text
<type>(<scope>): <简洁结果>
```

常用 type：`feat`、`fix`、`refactor`、`test`、`docs`、`chore`。描述应反映真实 diff，不写“update files”等空泛信息，也不把未验证结果写成已完成。

## 4. 暂存与提交

```bash
git add <具体文件一>
git add <具体文件二>
git diff --cached --name-only
git diff --cached
git commit -m "<message>"
```

逐文件暂存。若 staged 清单出现越界文件，使用非破坏的 `git restore --staged <具体文件>` 仅取消暂存，不丢弃工作区内容。

提交前完成变更对应的格式化、静态检查、测试、构建和 `git diff --check`。提交后读取 SHA 和状态，确认没有把其他会话文件纳入提交。

## 5. 远程操作

- fetch/读取远端是只读检查，按任务需要执行。
- pull/rebase 前确认当前脏工作区能安全处理；不得 stash 他者改动。
- push 仅在用户请求范围内执行，并通过 Tauri SSH MCP/Safe Credentials 处理认证。
- push 后读取远端分支/SHA 验证，不只依赖本地退出码。
- 强推、删除远端分支、重写 Tag/历史都是高风险动作，需要新的明确授权。
- 发布 Tag、Release 和产物上传交给 `release-publish`。

## 6. 冲突与禁止项

- 禁止 `git add .`、`git add -A`、`git stash`、`git reset --hard`、`git clean -fd`。
- 禁止 checkout/restore 丢弃工作区改动。
- 禁止为绕过冲突自动切分支、强推或覆盖 remote。
- 冲突涉及他者未提交文件时停止合并/变基并报告，不猜测应保留哪一侧。
- Git 操作失败时保留当前状态和错误证据，不继续执行依赖该结果的后续动作。


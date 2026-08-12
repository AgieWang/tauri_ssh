# 模板检测与更新

## 目录

- 只读基线
- 远端比较
- 更新决策
- Upstream 维护

## 只读基线

在模板仓库记录：

```bash
git status -s
git branch --show-current
git remote -v
git rev-parse HEAD
```

未提交文件视为用户或其他会话工作，不得被 stash、reset、checkout 或清理。新项目应基于用户确认的提交快照；若用户要求包含未提交内容，需改用明确的逐文件复制方案并说明差异。

## 远端比较

所有 Git 远端操作默认优先使用 Tauri SSH MCP。仅在 MCP 不可用且用户允许本地 Git 时执行：

```bash
git fetch origin
git rev-list HEAD..origin/master --count
git log HEAD..origin/master --oneline --no-merges
```

先展示当前提交、落后数量和提交摘要。`fetch` 只更新元数据；`pull` 会修改工作区，不得自动执行。

## 更新决策

- 模板已最新：继续使用当前提交。
- 模板落后且工作区干净：询问是否更新，记录更新前后提交。
- 模板有未提交工作：默认保持当前提交并避让，不自动更新。
- 远端分支不是 `master`：读取实际默认分支，不硬编码切换。
- 网络失败：允许继续用已确认的本地提交，但明确说明基线可能不是最新。

用户选择更新后，仍需检查快进条件和新 `git status -s`。出现冲突或需要 merge/rebase 时停止，不能在初始化流程中擅自解决模板分支历史。

## Upstream 维护

新项目可以将模板远端设为 `upstream`，用于后续只读比较：

```bash
git fetch upstream
git log master..upstream/master --oneline
git diff master...upstream/master -- src-tauri/src/
```

由于新项目已改名和替换标识，直接 merge 容易产生冲突。后续更新优先审查具体提交并选择性应用，不在初始化阶段自动 cherry-pick 或 merge。

---
name: release
description: 用于用户显式调用 /release、$release，或明确要求向指定仓库/渠道发布指定版本并编排完整流程；普通 build、打包或版本编辑不触发，也不自动授权外部写入。
---

# /release

`/release` 只负责编排完整发布顺序。普通“build”“构建”“打包”“版本”不触发；仅生成本地安装包使用 `tauri-packaging`，Updater 使用 `tauri-updater`，远端写入使用 `release-publish`。

配置字段、平台产物和阶段检查按需读取 [`references/release-orchestration.md`](references/release-orchestration.md)。

## 发布阶段

1. **只读预检**：读取当前分支/状态、版本源、发布配置、CI、签名与远端目标，确认未提交改动和当前发布基线。
2. **形成发布计划**：列出目标版本、版本文件、平台、产物、远端仓库、Tag、Updater manifest、验证门和回滚点。
3. **确认授权范围**：在首次本地写入、提交、push、Tag、创建 Release、修改外部 release 仓库前分别核对用户已明确授权的动作和目标。
4. **版本准备**：只在授权后更新版本与发布说明，并执行格式、测试、构建/配置一致性检查。
5. **发布前半段**：按 `release-publish` 执行提交、推送、Tag 和 CI 触发；具体打包和签名规则交给对应领域 Skill。
6. **CI 与产物验收**：等待并核对 commit、Tag、平台矩阵、签名文件和实际产物，不把 CI 启动或单个 job 成功当成发布完成。
7. **发布后半段**：经再次确认后复制产物、生成/校验 update manifest、写远端仓库并验证下载端点。
8. **交付报告**：列出已执行动作、版本/commit/Tag、产物校验、渠道状态、真实更新验证和残余风险。

## 外部写入授权边界

- **进入 `/release` 不自动授权外部写入。** 若用户的当前请求没有明确包含 exact push、Tag、Release、外部仓库或渠道，执行到对应门前必须停下确认。
- 不读取或输出明文私钥、token、密码；优先使用 Tauri SSH Safe Credentials、Git Workspace 和审批/审计链路。
- 不自动创建/覆盖 `.claude/release-config.json`；首次缺配置时先展示字段与目标路径，确认后再写。
- 不擅自变更目标平台、远端、主分支、签名方式或发布渠道。
- 不用 `git add -A`/`git add .`，不 stash/reset/clean，不覆盖其他会话的工作。

## 验证与回滚

- 版本号必须在所有规范源中一致；产物文件名、架构、签名和 update manifest URL 必须与配置一致。
- Tag 前保留可回滚点；Tag/push 后不得本地假装回滚，必须说明远端状态并请求用户决定修复版本、撤销 Release 或删除 Tag。
- 任一目标仓库 push 失败时停止后续渠道写入，保留成功/失败矩阵，不重复盲推。
- 发布完成至少需要：目标 commit/Tag 可追溯、平台产物齐全、签名/manifest 校验通过、下载端点可访问；真实客户端更新仍应单独验收。

## 不应触发

- “运行 pnpm build”——普通构建验证。
- “生成一个 dmg”——使用 `tauri-packaging`。
- “把 package.json 版本改为 1.2.0”——普通本地版本编辑，不授权发布。

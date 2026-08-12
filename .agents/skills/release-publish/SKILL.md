---
name: release-publish
description: |
  用于用户明确授权后的 Git push/tag、Release 上传、远端更新仓库写入和正式版本发布；这是外部写入终端动作，不因“发布”或“release”单个词自动执行。

  触发场景：
  - 明确要求发布指定版本并推送源码、Tag 或 Release
  - 明确要求上传签名产物或推送远端 `update.json`
  - 明确要求执行 Gitee/GitHub 双仓库发布终端流程
  - 正式发布前核对版本、签名、凭据、审批和回滚门禁

  触发词：执行正式发布、推送 release tag、上传 Release 产物、推送 update.json、发布到 Gitee、发布到 GitHub、外部发布、版本上线
---

# 正式发布与外部写入

## 终端动作边界

本技能负责发布链路中的外部状态变更。普通打包使用 `tauri-packaging`；自动更新协议使用 `tauri-updater`；仅创建提交/分支使用 `git-workflow`；完整 `/release` 编排由 `release` 调用本技能。

讨论发布方案、生成检查清单、修改本地版本文件、出现“发布/release”字样，均不自动授权 push、tag、上传、创建 Release 或修改远端仓库。缺少明确版本、目标远端或执行授权时，只能准备和报告。

## 不可下沉的发布门禁

1. 明确发布版本、目标平台、目标远端、是否草稿、更新说明和授权动作；不得自行扩大到其他仓库/平台。
2. 先确认工作区、分支、提交范围、上游、CI、版本一致性和已有 Tag/Release；保留其他会话改动。
3. Git、远端服务器和凭据操作优先使用 Tauri SSH MCP/Safe Credentials；不得输出 Token、密码、私钥或签名密钥。
4. 逐文件暂存，只提交本任务文件；禁止 `git add .`、`git add -A`、stash、reset hard、clean 或丢弃他人改动。
5. 推送、Tag、Release、产物上传和 update manifest 每一步都要验证远端结果；本地命令成功不等于发布成功。
6. 签名、版本、平台、URL、产物和 manifest 必须一一对应；不使用缺失/过期签名继续发布。
7. 发布失败立即停止后续外部写入，记录已完成步骤，并使用预先定义的非破坏回滚/修复路径。

## 执行流程

1. 进入准备态：读取 [references/release-gates.md](references/release-gates.md)，完成只读门禁和变更范围核对。
2. 只有门禁通过且授权清楚，才按最小步骤提交/推送/Tag，并等待真实 CI 结果。
3. CI 成功后，核验签名产物，再按已授权平台生成 manifest 和上传目标。
4. 逐个远端验证 Tag、Release、文件、URL、签名和 manifest 可访问性。
5. 输出已完成、未完成、失败点、远端证据和可恢复方式；不得把排队中或 HTTP 200 单点证据表述为生产验收。

## 按需参考

- 每次正式发布都必须读取 [references/release-gates.md](references/release-gates.md)。
- 需要平台矩阵、首次配置、Gitee/GitHub 命令、产物处理、CI 或故障排查时，再读取 [references/platform-publish.md](references/platform-publish.md)。
- 若仅打安装包或配置 updater，不读取平台发布命令，更不执行外部写入。

## 完成条件

- 授权范围、版本、目标、凭据和回滚路径明确。
- 本地检查、签名、CI、产物、远端 Tag/Release/manifest 均有真实证据。
- 未泄露秘密，未提交或覆盖其他会话文件。
- 发布结果与剩余风险清晰记录；`git diff --check` 及适用构建/测试通过。

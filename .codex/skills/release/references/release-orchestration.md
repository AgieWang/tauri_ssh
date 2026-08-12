# 发布编排参考

仅在显式 `/release` 且相应阶段需要时读取。这里的字段和命令不是默认授权。

## 发布配置最小字段

```json
{
  "appName": "<from tauri.conf.json>",
  "sourceRemote": "<remote name>",
  "sourceRepoUrl": "<repository url>",
  "releaseTargets": [
    { "name": "<target>", "repoUrl": "<url>", "localPath": "<absolute path>" }
  ],
  "platforms": ["windows", "macos"],
  "mainBranch": "<main or master>"
}
```

首次配置必须逐项确认。绝对路径、远端 URL、分支和平台不能从示例推断。

## 平台产物

| 平台 | 常见安装产物 | Updater 产物 |
|---|---|---|
| Windows | `.exe` 或 `.msi` | 安装/压缩包及对应 `.sig` |
| macOS ARM/Intel | 对应架构 `.dmg` | `.app.tar.gz` 及 `.sig` |
| Linux | `.AppImage`、`.deb` 等 | AppImage/压缩包及 `.sig` |

实际清单以 `.github/workflows/release.yml`、`tauri.conf.json` 和本次 CI 输出为准，不能套用旧模板。

## 阶段门

### 版本准备

- 读取 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`package.json` 和锁文件的版本关系。
- 核对 changelog/README 的真实存放位置，不默认存在两个 release 仓库。
- 运行项目要求的格式、类型、测试、构建和 `git diff --check`。

### Tag 与 CI

- Tag 必须指向已确认的源码 commit。
- push 前读取远端和分支，不用示例 remote 名。
- CI 需要核对所有目标平台 job、签名步骤、上传文件和 Release 附件。

### Updater 与发布仓库

- 从实际 `.sig` 读取签名，不能手写或复用旧签名。
- update manifest 仅包含本次支持平台；URL、版本、日期、notes 与发布结果一致。
- 多渠道逐个写入和验证，保留每个渠道的成功/失败结果。

## 回滚决策

| 阶段 | 可选回滚 |
|---|---|
| 仅本地文件、未提交 | 仅撤销本次明确改动；不得覆盖原有未提交内容 |
| 已提交、未 push | 经用户确认创建修复提交或重做本地提交 |
| 已 push、未 Tag | 使用新的修复提交，不改写共享历史 |
| 已 Tag/Release | 停止并说明远端状态，由用户选择修复版本或远端撤销动作 |
| 已发布 manifest | 优先发布修复版本；任何撤回/覆盖都需明确授权和客户端影响评估 |

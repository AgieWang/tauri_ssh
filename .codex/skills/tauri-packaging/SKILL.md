---
name: tauri-packaging
description: |
  用于生成和验证 Tauri 安装包、bundle、代码签名与跨平台产物；普通前端构建、类型检查或 Rust 编译不触发。

  触发场景：
  - 生成 MSI、NSIS、DMG、AppImage、DEB 或应用 bundle
  - 配置 Tauri bundle 元数据、图标、架构或平台安装参数
  - 执行或诊断 macOS/Windows 代码签名与公证
  - 验证安装包体积、产物目录、可安装性和启动行为

  触发词：Tauri bundle、pnpm tauri build、MSI、NSIS、DMG、AppImage、代码签名、公证、安装包、bundle targets
---

# Tauri 打包与安装包

## 边界

本技能只负责本地或 CI 的安装包构建与验证。`pnpm build`、`tsc`、`cargo check` 的普通构建错误使用诊断/构建工具；上传 Release、推送 Tag/update.json 或远程发布使用 `release-publish`；自动更新清单与 updater 签名契约使用 `tauri-updater`。

“构建”“build”“发布”单独出现时不足以触发，必须存在 Tauri bundle、安装包、签名或具体平台产物意图。

## 强制规则

1. 先确认目标平台、架构、bundle 类型、版本、签名要求和交付位置。
2. 读取当前 `tauri.conf.json`、Cargo/package 版本、CI workflow 和图标配置；不使用过时的通用命令覆盖项目配置。
3. 签名私钥、密码和证书不得写入仓库、命令输出或聊天；使用受控 Secrets/Safe Credentials。
4. 构建产物不等于发布授权。未明确要求远程发布时，不 push、不 tag、不上传、不改远端 Release。
5. 不为缩小体积或修复平台问题盲目删除依赖；先测量，再做可回滚的最小修改。
6. 本机不能代表目标平台时，使用项目 CI 验证，不伪称跨平台已通过。
7. NSIS 安装语言服从产品要求：只有明确要求所有 Windows 系统强制中文时才只配置 `SimpChinese` 并关闭选择器；多语言产品不得套用该默认。

## 执行流程

1. 核对版本一致性、依赖锁、bundle 配置、目标矩阵和签名条件。
2. 选择最小目标 bundle 并执行对应构建；保留完整失败日志和产物路径。
3. 校验文件名、架构、签名/公证、安装、启动、卸载和体积。
4. 若产物供 Updater 使用，再交给 `tauri-updater` 验证签名与 manifest。
5. 只有用户明确授权发布时，才交给 `release-publish` 执行外部写入。

## 按需参考

涉及具体平台命令、配置、图标、体积或输出目录时读取 [references/platform-bundles.md](references/platform-bundles.md)。仅排查普通 `pnpm build`/`cargo check` 时不得加载或执行其中命令。

## 完成条件

- 目标平台、架构和产物类型明确，版本与配置一致。
- 产物真实生成并完成适用的签名、安装和启动验证。
- 没有暴露签名材料，也没有越权执行外部发布。
- 相关检查及 `git diff --check` 通过。

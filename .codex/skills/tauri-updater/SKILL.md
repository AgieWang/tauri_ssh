---
name: tauri-updater
description: |
  用于 Tauri 应用自动更新、Updater 插件、更新端点、签名验证和 update manifest；普通代码、依赖或文档更新不触发。

  触发场景：
  - 集成或修改 `tauri-plugin-updater` 与更新检查 UI
  - 配置 updater endpoints、公钥或平台更新通道
  - 生成、校验或诊断 `update.json`/静态更新 manifest
  - 处理更新包签名、下载、安装、重启与失败回滚

  触发词：tauri-plugin-updater、updater endpoints、update.json、更新签名、OTA、自动更新、更新公钥、更新清单、install update
---

# Tauri 自动更新

## 边界

本技能处理“已安装应用如何安全升级”。普通修改文件、升级 npm/crate、更新业务数据或“更新一下代码”不应触发。生成安装包使用 `tauri-packaging`；推送 Tag、上传产物、写远端更新仓库使用 `release-publish`。

## 不可下沉的更新安全规则

1. 更新必须验证签名与版本，端点使用受控 HTTPS；失败时拒绝安装，禁止跳过签名。
2. 私钥和密码只存在于受控 Secrets/Safe Credentials，不写入仓库、manifest、日志或聊天；客户端只配置公钥。
3. manifest 中版本、平台、架构、下载 URL、签名和产物必须一一对应，不能复用错误签名。
4. 生成本地 manifest 不代表有权上传。没有明确外部写入授权时，不推送、不发布、不改远端更新源。
5. 必须设计下载失败、签名失败、安装失败、重启失败和旧版本兼容策略；更新 UI 不得把失败伪装成成功。

## 执行流程

1. 读取当前插件注册、Capabilities、`tauri.conf.json`、版本源、CI 和更新 UI。
2. 确认目标平台/架构、endpoint、manifest 格式、公钥和签名产物来源。
3. 实现检查、确认、下载、安装、重启及错误传播；通过统一 API 封装调用。
4. 用可控测试端点验证无更新、有更新、平台不匹配、坏签名、断网和回滚提示。
5. 若要构建或发布产物，分别交给 `tauri-packaging` 和 `release-publish`，再次核对授权。

## 按需参考

涉及依赖安装、endpoint 配置、manifest 字段、密钥生成、CI 模板或签名命令时读取 [references/signing-and-manifest.md](references/signing-and-manifest.md)。读取时不得输出其中可能对应真实环境的秘密路径或值。

## 完成条件

- 版本、平台、URL、签名和产物已逐项一致性校验。
- 正常更新及坏签名/断网/不匹配等拒绝路径真实验证。
- 密钥未泄露，外部发布动作有明确授权与审计证据。
- 相关测试、构建、UTF-8 和 `git diff --check` 通过。

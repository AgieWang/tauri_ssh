# Updater 签名与 Manifest 参考

## 目录

1. 集成链路
2. Endpoint 与配置
3. Manifest 契约
4. 前端更新流程
5. 签名材料
6. CI 与发布衔接
7. 验证场景

## 1. 集成链路

自动更新需要同时存在：

1. Rust `tauri-plugin-updater` 依赖与 Builder 注册；
2. 对应前端插件包；
3. 最小 updater Capability；
4. `tauri.conf.json` 的 endpoint 和公钥；
5. 目标平台 updater bundle 与 `.sig`；
6. 可访问且字段正确的 manifest；
7. 前端检查、确认、下载、安装和重启交互。

只完成其中一层不算集成完成。版本、平台、架构和字段名以当前插件 schema/项目配置为准。

## 2. Endpoint 与配置

Endpoint 使用受控 HTTPS，URL 模板中只包含插件支持的变量。生产和测试更新源分离，开发端点不得进入正式配置。

```json
{
  "plugins": {
    "updater": {
      "endpoints": ["https://updates.example.invalid/channel/update.json"],
      "pubkey": "<public-key>"
    }
  }
}
```

客户端只持有公钥。Endpoint 失败、重定向到未允许 host、TLS 错误或响应超限时必须失败关闭。

## 3. Manifest 契约

常见静态 manifest 包含：

```json
{
  "version": "1.2.3",
  "notes": "更新说明",
  "pub_date": "2026-08-01T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "url": "https://updates.example.invalid/app.exe",
      "signature": "<base64-signature>"
    }
  }
}
```

只输出当前实际启用的 target。对每个 target：

- URL 指向 updater 产物而非人工安装产物；
- `.sig` 与该产物一一对应；
- 签名内容进行 Base64 解码/重编码校验；
- manifest 生成后重新解析并与原始 `.sig`、版本和 URL 比较；
- 公共 URL 验证状态、大小及可用 hash。

不要用字符串替换生成 JSON，不人工粘贴签名，不使用宽 glob 查找 `.sig`。

## 4. 前端更新流程

更新 UI 至少处理：

```text
检查中 -> 无更新
      -> 有更新 -> 展示版本/说明 -> 用户确认
                              -> 下载进度
                              -> 安装成功 -> 用户确认重启
                              -> 失败 -> 可诊断错误与重试
```

- 应用启动时检查需避免阻塞首屏，可配置退避和检查频率。
- 用户触发检查应显示明确加载/成功/失败状态。
- 下载、安装和 relaunch 错误不可吞掉；重启前处理未保存数据。
- 多次点击要去重，组件卸载要停止 UI 更新或清理监听。
- TypeScript 类型明确，调用经 `src/lib/api/` 统一封装。

## 5. 签名材料

使用 Tauri 当前版本提供的 signer 工具生成密钥，但在执行前确认输出目标是安全、已排除版本控制的具体位置。

- 私钥和密码放入 CI Secrets/Safe Credentials；不显示、不复制到聊天、不写 Shell 历史。
- 公钥可写入客户端配置；不要混淆公钥与私钥。
- 多个 CI remote 需要一致且独立配置 Secrets，不能静默退回未签名或临时密钥。
- 密钥轮换影响所有已安装客户端，必须形成兼容与回滚方案并获得明确授权。
- 签名生成后逐 target 验证，不能仅凭 CI job 绿色判断正确。

## 6. CI 与发布衔接

CI 负责的边界由当前 workflow 决定，常见职责是构建、签名并上传草稿 Release。外部更新仓库/CDN 写入由 `release-publish` 执行。

CI 全部成功和产物完整预检前，不发布 manifest。产物集合按启用 target 动态计算；每个 App/版本使用独立目录，防止旧 `.sig` 或其他项目文件污染。

Updater 任务若只修改本地代码/配置，不自动授权打 Tag、上传或推送 update.json。

## 7. 验证场景

- 当前版本无更新；
- 新版本正常检查、下载、安装和重启；
- endpoint 断网、超时、HTTP 错误和无效 JSON；
- 平台/架构缺失或 URL 指向错误产物；
- 空签名、坏 Base64、签名与产物不匹配；
- 版本相等、版本回退和不兼容旧客户端；
- 用户取消、重复点击、下载中退出和未保存数据；
- 测试/生产通道隔离以及真实发布 URL 回读。


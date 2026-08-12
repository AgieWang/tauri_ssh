# 原生桌面通知实现参考

仅在接入或调用 `tauri-plugin-notification` 时读取。

## 依赖与注册

版本以当前 `Cargo.toml`、`package.json` 和插件文档为准：

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_notification::init())
```

Capabilities 只声明实际需要的 notification 权限。权限标识随插件版本核对，不照搬历史示例。

## TypeScript 权限与发送

```typescript
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export async function sendTaskCompletedNotification(
  title: string,
  body: string,
): Promise<"sent" | "denied"> {
  let granted = await isPermissionGranted();
  if (!granted) {
    const permission = await requestPermission();
    granted = permission === "granted";
  }
  if (!granted) return "denied";

  sendNotification({ title, body });
  return "sent";
}
```

权限请求应由用户理解的操作触发。应用启动时无上下文弹权限会降低授权率并打扰用户。

## Rust 发送

```rust
use tauri_plugin_notification::NotificationExt;

pub fn notify_task_complete(
    app: &tauri::AppHandle,
    title: &str,
    body: &str,
) -> Result<(), AppError> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| AppError::Custom(error.to_string()))
}
```

若通过 Command 暴露，不允许前端任意传入无限制标题/body 形成通知滥用；Service 校验长度、频率和敏感信息。

## 页面提示不是原生通知

简单操作反馈使用项目已有 Ant Design：

```typescript
import { message } from "antd";

message.success("保存成功");
message.error("保存失败");
```

不要为页面 Toast 额外引入 `react-hot-toast`，也不要因此加载本 Skill。

## 频率治理

- 同一任务/事件使用稳定 dedupe key。
- 短时间大量事件聚合为一条摘要。
- 用户可配置通知类别、成功/失败级别和免打扰。
- 后台轮询错误采用退避，不能每轮都通知。
- 通知发送失败只影响提醒，不把已成功的核心任务改成失败。

## 敏感内容

锁屏可能展示通知。禁止包含密码、Token、私钥、完整服务器地址、数据库连接串、客户敏感数据和本地敏感路径。需要详情时显示概括并引导用户打开应用内受控页面。

## 点击和深链

- 只允许应用内白名单路由和标识符。
- 校验通知关联对象仍存在且用户仍有权限。
- 处理应用未启动、后台、前台和多窗口状态。
- 不直接打开通知正文提供的任意 URL 或文件路径。

## 验证矩阵

| 场景 | 期望 |
|---|---|
| 首次请求允许 | 发送并正常展示 |
| 用户拒绝 | 不重复骚扰，应用内可解释 |
| 权限已允许 | 不重复请求 |
| 插件/系统发送失败 | 核心任务仍成功，错误可诊断 |
| 高频重复事件 | 去重/聚合生效 |
| 前台/后台/应用未启动 | 行为符合产品设计 |
| 点击通知 | 仅打开白名单目标 |

原生通知必须在真实 Tauri 桌面运行环境验证，Vite 浏览器页面不足以证明系统通知可用。


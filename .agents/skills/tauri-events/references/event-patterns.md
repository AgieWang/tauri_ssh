# Tauri 事件通信模式参考

## 目录

1. Event 与 Command 选择
2. Rust 发送事件
3. React 监听与清理
4. 进度与窗口定向事件
5. 事件设计检查表

## 1. Event 与 Command 选择

| 需求 | 首选 |
|---|---|
| 前端发起并需要结果/错误 | Command `invoke` |
| Rust 主动推送进度或异步状态 | Event |
| 已知目标窗口的通知 | 定向 Event |
| 大数据流或需要背压 | Channel/专门流式模式 |

事件没有天然响应值。需要确认、事务或强错误语义时，不要用两个事件模拟 Command。

## 2. Rust 发送事件

载荷定义为可序列化类型，事件错误传播到 Command：

```rust
use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    current: usize,
    total: usize,
}

#[tauri::command]
async fn process_items(window: tauri::Window, total: usize) -> Result<(), String> {
    for current in 1..=total {
        window
            .emit("job-progress", ProgressPayload { current, total })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
```

后台循环无法把错误返回给已结束的 Command 时，应记录一次带关联 ID 的错误并停止/退避，不能 `unwrap()`：

```rust
if let Err(error) = app_handle.emit("heartbeat", "alive") {
    log::error!("发送 heartbeat 失败: {error}");
    break;
}
```

## 3. React 监听与清理

处理异步注册和组件提前卸载：

```tsx
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

interface ProgressPayload {
  current: number;
  total: number;
}

useEffect(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;

  void listen<ProgressPayload>("job-progress", (event) => {
    if (!disposed) setProgress(event.payload);
  }).then((cleanup) => {
    if (disposed) cleanup();
    else unlisten = cleanup;
  }).catch((error: unknown) => {
    message.error(String(error));
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}, []);
```

监听器不能在每次渲染重复注册。事件回调中的异步任务还需处理组件卸载、乱序和过期响应。

## 4. 进度与窗口定向事件

- 进度载荷至少包含关联 ID、current、total 和阶段；多个任务并发时用关联 ID 过滤。
- 事件频率高时节流或合并，避免每条记录触发一次 React 渲染。
- 只通知特定窗口时使用 `emit_to`/明确 `EventTarget`；窗口 label 来自窗口管理约定。
- 定义 completed/failed/cancelled 终止语义，避免 UI 永远停留在 loading。
- 取消必须能传回后台任务；单纯卸载监听器不会自动停止 Rust 工作。
- 事件名集中为常量或类型，不在多个文件散写相似字符串。

## 5. 事件设计检查表

- 方向、目标、频率、顺序和幂等语义明确；
- Rust/TypeScript 载荷字段和命名完全一致；
- `emit` 失败可见，无 `unwrap()`/静默丢弃；
- React 在卸载、重复挂载和窗口关闭时释放监听；
- 高频事件有节流/背压策略；
- 敏感信息不进入广播载荷；
- 一次、并发、取消、失败、窗口关闭和重连场景均测试。


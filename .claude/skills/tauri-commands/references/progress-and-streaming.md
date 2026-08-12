# 进度与流式模式

## 目录

1. 选择模式
2. 事件进度
3. Channel/stream
4. 清理与并发
5. 验证

## 1. 选择模式

| 需求 | 建议 |
|---|---|
| 一次请求一次结果 | 普通 Command 返回值 |
| 少量阶段进度或跨组件通知 | Tauri event |
| 高频、按调用绑定的数据流 | Tauri 2 Channel 或等价受控流 |
| 可取消后台任务 | task id + 独立状态/取消 Command |

不要为单次返回引入事件；不要用同一个无 task id 的全局事件承载多个并发任务。

## 2. 事件进度

后端 payload 使用可序列化结构，事件名带业务域：

```rust
use tauri::Emitter;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
    pub task_id: String,
    pub current: usize,
    pub total: usize,
    pub path: String,
}

#[tauri::command]
pub async fn process_files(
    window: tauri::Window,
    task_id: String,
    files: Vec<String>,
) -> Result<usize, CommandError> {
    let total = files.len();
    let mut succeeded = 0;
    for (index, path) in files.into_iter().enumerate() {
        FileService::process(&path).await?;
        succeeded += 1;
        window.emit("files://progress", FileProgress {
            task_id: task_id.clone(),
            current: index + 1,
            total,
            path,
        })
        .map_err(|error| AppError::Custom(error.to_string()))?;
    }
    Ok(succeeded)
}
```

事件 payload 不发送密码、Token、私钥、完整命令行或不必要的本地路径。

## 3. 前端监听

先建立监听再启动任务，所有路径都清理：

```typescript
const unlisten = await listen<FileProgress>("files://progress", (event) => {
  if (event.payload.taskId === taskId) {
    setProgress(event.payload);
  }
});

try {
  return await fileApi.processFiles({ taskId, files });
} finally {
  unlisten();
}
```

React 组件可能在任务结束前卸载时，用 `useEffect` cleanup 保存并调用 `UnlistenFn`，并防止卸载后更新状态。

## 4. Channel/stream

高频数据优先使用当前 Tauri 2 版本支持的 Channel API，避免事件总线广播压力。实现前必须读取仓库依赖版本和官方 API，不凭旧示例猜签名。

Channel 设计仍需：

- payload 类型稳定并可版本化；
- 明确完成、错误、取消三种终态；
- 发送端处理接收端消失；
- 限制速率和缓冲，避免生产者拖垮 UI；
- 每次调用独立 channel，不混流。

## 5. 批处理失败策略

批处理不应只用一个数字掩盖失败。根据业务返回：

```rust
#[derive(serde::Serialize)]
pub struct BatchResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<ItemFailure>,
}
```

若要求事务性，失败时由 Service/Database 回滚；若允许部分成功，返回逐项错误且避免把敏感内部细节展示给 UI。

## 6. 验证

- 启动监听早于 Command，结束和错误后监听数量恢复。
- 两个并发任务只更新各自 UI。
- 0 项、1 项、大批量、单项失败和取消路径正确。
- 进度不超过 total，不出现除零或倒退。
- 窗口关闭、路由切换后无内存泄漏和未处理 Promise。
- 浏览器验收控制台无重复监听警告；Tauri 运行时验证真实事件或 Channel。

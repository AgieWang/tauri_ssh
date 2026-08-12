# 注入与异步模式

## 目录

1. 模块和薄 Command
2. State/AppHandle/Window 注入
3. async 与阻塞任务
4. 锁、取消和错误

## 1. 模块和薄 Command

按业务域拆分 `commands/*.rs`，并在 `commands/mod.rs` 导出。Command 只做边界工作：

```rust
#[tauri::command]
pub fn get_all_config(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AppConfig>, CommandError> {
    ConfigService::get_all(&state.db).map_err(Into::into)
}
```

不要把所有 Command 写进 `lib.rs`，也不要在 Command 中直接写事务和 SQL。是否需要 Service/Database 以当前模块为准，纯计算无需空造层。

## 2. State/AppHandle/Window 注入

### State

```rust
#[tauri::command]
pub fn list_keys(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
    ConfigService::list_keys(&state.db).map_err(Into::into)
}
```

共享可变值按当前 `AppState` 的锁模型访问。锁中毒和借用失败必须转为错误，不得 `unwrap()`。

### AppHandle

```rust
use tauri::Manager;

#[tauri::command]
pub fn app_data_dir(app: tauri::AppHandle) -> Result<String, CommandError> {
    app.path()
        .app_data_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| AppError::Custom(error.to_string()))
        .map_err(Into::into)
}
```

用于应用路径、资源、全局状态或跨窗口能力。不要接受前端传入的任意本地路径替代安全路径解析。

### Window / WebviewWindow

```rust
#[tauri::command]
pub fn set_current_title(window: tauri::Window, title: String) -> Result<(), CommandError> {
    if title.trim().is_empty() {
        return Err(AppError::InvalidInput("标题不能为空".into()).into());
    }
    window
        .set_title(&title)
        .map_err(|error| AppError::Custom(error.to_string()))
        .map_err(Into::into)
}
```

具体窗口类型以 Tauri 2 当前 API 和仓库相似实现为准。窗口创建、scope 和多窗口生命周期进一步使用 `tauri-window-management`。

### 组合注入

框架对象不属于前端 payload：

```rust
#[tauri::command]
pub async fn run_for_user(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    user_id: String,
) -> Result<TaskResult, CommandError> {
    TaskService::run(&app, &window, &state.db, &user_id).await
        .map_err(Into::into)
}
```

参数顺序不是契约本身；依赖宏解析和具体类型。保持项目统一顺序有助于可读性，但不要声称改变顺序会必然失败。

## 3. async 与阻塞任务

### 异步 IO

```rust
#[tauri::command]
pub async fn fetch_metadata(url: String) -> Result<Metadata, CommandError> {
    validate_allowed_url(&url)?;
    MetadataService::fetch(&url).await.map_err(Into::into)
}
```

- 网络、异步文件 API、异步子进程使用 async。
- 禁止前端提供任意 URL 后由 Rust 无限制请求；执行 SSRF/allowlist 检查。
- 不在 async Command 中用 `std::thread::sleep`。

### CPU 或阻塞库

同步数据库库、压缩、解析等可能阻塞时，先测量耗时；需要迁出 async 调度线程时：

```rust
#[tauri::command]
pub async fn parse_large_file(path: String) -> Result<Summary, CommandError> {
    let result = tokio::task::spawn_blocking(move || ParserService::parse(path))
        .await
        .map_err(|error| AppError::Custom(format!("后台任务失败: {error}")))??;
    Ok(result)
}
```

不要为了形式给所有同步快速查询套 `spawn_blocking`；依据真实负载和当前数据库线程模型决定。

## 4. 锁、取消和错误

- 不持有 `MutexGuard`/数据库连接锁跨 `.await`。
- 长任务设计任务 ID、取消信号和结束状态；窗口关闭后应停止无用工作或安全丢弃事件。
- 批处理失败策略要明确：全失败回滚、部分成功汇总，或遇错即停。
- 错误返回稳定的 `code` 和可读 `message`；日志可含内部上下文，但不得含凭据。
- async task join error、panic 和取消要区分，不能全部降级成“未知错误”。

验证时至少覆盖：正常、非法参数、依赖错误、取消、并发两个任务，以及窗口/应用生命周期变化。

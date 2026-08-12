# Tauri Command 完整实现参考

## 目录

1. 模块组织与注册
2. 参数和错误
3. 批量操作
4. 子进程安全
5. 完整检查清单

## 1. 模块组织与注册

```text
src-tauri/src/
├── commands/<domain>.rs     # IPC 边界
├── services/<domain>.rs     # 业务逻辑
├── database/<domain>.rs     # SQL/DAO（按需）
├── models/<domain>.rs       # 请求/响应/领域模型
└── lib.rs                   # handler 注册
```

```rust
// commands/mod.rs
pub mod config;
pub mod system;

// lib.rs
.invoke_handler(tauri::generate_handler![
    commands::config::get_config,
    commands::config::set_config,
    commands::system::get_system_info,
])
```

Command 必须 `pub` 才能跨模块注册。注册顺序不影响调用，但按业务域分组更易审查。端到端 API 和类型部分读取 `api-development` references。

## 2. 参数和错误

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchConfigInput {
    pub configs: Vec<ConfigEntry>,
}

#[tauri::command]
pub fn set_batch_config(
    state: tauri::State<'_, AppState>,
    input: BatchConfigInput,
) -> Result<BatchResult, CommandError> {
    if input.configs.is_empty() {
        return Err(AppError::InvalidInput("配置列表不能为空".into()).into());
    }
    ConfigService::set_batch(&state.db, input.configs).map_err(Into::into)
}
```

输入校验放在 Command 或 Service 的统一入口；跨多个调用复用的业务不变量放 Service。不要在 Command 文件定义仅为“方便”的重复领域模型。

结构化错误示意：

```rust
#[derive(Debug, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}
```

优先复用仓库现有定义与转换，不能复制一个同名类型。

## 3. 批量操作

简单循环写入可能产生部分成功，先明确业务语义：

- 全部成功才提交：Service 调用 Database 事务。
- 允许部分成功：收集每项结果并返回汇总。
- 长批量：async + 进度/Channel + 可取消 task id。

```rust
pub fn set_batch(
    db: &Database,
    configs: Vec<ConfigEntry>,
) -> Result<BatchResult, AppError> {
    db.with_transaction(|tx| {
        for config in &configs {
            tx.set_config(&config.key, &config.value)?;
        }
        Ok(BatchResult::all_succeeded(configs.len()))
    })
}
```

实际事务 API 以当前 Database 实现为准，不引入不存在的抽象。

## 4. 子进程安全

### 通用要求

- 程序名来自固定 allowlist 或可信配置，不由用户任意输入。
- 使用 `.arg()`/`.args()` 传参数，禁止 `sh -c`、`cmd /C` 拼接未验证输入。
- 设置超时、退出码检查、stdout/stderr 大小限制和 UTF-8 降级。
- 不在命令行和日志中传密码、Token、私钥。
- 多处执行时抽取经过审查的 helper，但不要隐藏权限和审计。

### Windows 同步 Command 构造（仅展示防弹窗）

```rust
enum KnownTool {
    Git,
}

fn version_command(tool: KnownTool) -> std::process::Command {
    // 程序来自枚举 allowlist，不能接收任意字符串。
    let program = match tool {
        KnownTool::Git => "git",
    };
    let mut command = std::process::Command::new(program);
    command.arg("--version");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
```

这段代码只展示 allowlist 和 Windows 标志，**禁止**在同步 IPC Command 中直接调用 `.output()`。执行时使用下方有超时和输出上限的异步 helper；必须保留同步库时，将同等超时控制放入 `spawn_blocking` 的受审 helper。

### 异步 tokio::process::Command

```rust
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);

async fn read_limited<R>(reader: R) -> Result<Vec<u8>, AppError>
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader.take(MAX_OUTPUT_BYTES + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err(AppError::Custom("子进程输出超过 1 MiB 限制".into()));
    }
    Ok(bytes)
}

async fn run_bounded(
    mut command: tokio::process::Command,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), AppError> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Custom("无法读取子进程 stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Custom("无法读取子进程 stderr".into()))?;
    let stdout_task = tokio::spawn(read_limited(stdout));
    let stderr_task = tokio::spawn(read_limited(stderr));

    let status = match tokio::time::timeout(PROCESS_TIMEOUT, child.wait()).await {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(AppError::Custom("子进程执行超时".into()));
        }
    };
    let stdout = stdout_task
        .await
        .map_err(|error| AppError::Custom(format!("读取 stdout 失败: {error}")))??;
    let stderr = stderr_task
        .await
        .map_err(|error| AppError::Custom(format!("读取 stderr 失败: {error}")))??;
    Ok((status, stdout, stderr))
}

async fn npm_view(package: &str) -> Result<String, AppError> {
    validate_package_name(package)?;
    let program = if cfg!(target_os = "windows") { "npm.cmd" } else { "npm" };
    let mut command = tokio::process::Command::new(program);
    command.args(["view", package, "--json"]);
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let (status, stdout, _stderr) = run_bounded(command).await?;
    if !status.success() {
        return Err(AppError::Custom("npm 查询失败".into()));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}
```

`std::process::Command::creation_flags` 需要 `std::os::windows::process::CommandExt`；`tokio::process::Command` 的具体 trait/方法可见性以当前 tokio 版本编译结果为准。

## 5. 完整检查清单

- [ ] 读取相似 Command、Service、错误类型和 `Cargo.toml`。
- [ ] Command 名称未冲突，参数已验证，框架对象由 Tauri 注入。
- [ ] Command 薄层，Service 负责业务，Database 负责 SQL。
- [ ] sync/async 与负载匹配，无阻塞 sleep，无锁跨 await。
- [ ] 长任务有进度、终态、取消和并发隔离。
- [ ] 子进程无 shell 拼接、无凭据泄漏、检查退出码，Windows 不弹窗。
- [ ] `commands/mod.rs` 和 `generate_handler![]` 已注册。
- [ ] 错误可序列化且前端可稳定解析。
- [ ] 格式化、聚焦测试、cargo check/clippy 和 diff check 通过。

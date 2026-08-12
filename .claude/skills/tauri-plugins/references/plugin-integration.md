# Tauri 插件集成与开发参考

## 目录

1. 集成前决策
2. 官方插件集成链路
3. Service 与前端边界
4. Capability 联动
5. 自定义插件
6. 平台与故障验证

## 1. 集成前决策

先确认需求确实属于 Tauri 插件，而不是普通 crate/npm 包或现有 Command 能力。检查：

- 当前 `Cargo.toml`、`package.json`、锁文件已有依赖；
- 当前 `src-tauri/src/lib.rs` 已注册插件；
- `src-tauri/capabilities/` 已声明权限；
- Tauri 主版本、Rust crate 和前端包的兼容版本；
- 官方维护状态、许可证、目标平台和移动端支持。

不要依赖静态“完整插件清单”，以当前官方文档和仓库锁文件为准。

## 2. 官方插件集成链路

完整链路通常包含四处：

```text
Cargo.toml: tauri-plugin-<name>
  -> lib.rs: Builder.plugin(...)
  -> capabilities/*.json: 最小 permission/scope
  -> package.json + src/lib/api/: 前端绑定与类型封装
```

示例骨架仅表示结构，初始化方式以插件当前 API 为准：

```rust
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!());

    if let Err(error) = result {
        log::error!("Tauri 启动失败: {error}");
    }
}
```

依赖、注册、权限和前端包缺一都会导致编译或运行时不可用。修改 lock 文件后检查是否连带升级无关依赖。

## 3. Service 与前端边界

选择调用位置：

| 场景 | 建议 |
|---|---|
| 纯 UI 且插件 JS API 已满足 | `src/lib/api/` 封装插件调用 |
| 需要凭据、审计、业务校验 | Rust Command -> Service -> 插件 API |
| 多处复用或组合数据库/文件操作 | Service 层统一编排 |
| 插件需要 AppHandle | Command 注入或 Service 接收最小上下文 |

页面不得散落裸插件调用。TypeScript 封装明确参数、返回和错误；敏感输入在 Rust 再校验。

## 4. Capability 联动

1. 从当前生成 schema/插件权限文档确认 permission identifier。
2. 只声明实际使用的命令；需要 scope 时限制窗口、文件、URL 或命令目标。
3. 在真实 Tauri 运行时验证允许与拒绝路径。
4. permission/scope 细节读取 `tauri-capabilities`；凭据、Shell、文件和网络高风险读取 `security-permissions`。

不能为排查插件问题直接开放全量 default 或宽通配。

## 5. 自定义插件

自定义插件只有在能力需要跨项目复用、平台实现独立或必须暴露插件式 API 时采用。项目内单一业务功能优先现有 Command/Service 分层。

自定义插件至少定义：

- Rust 插件入口、状态和错误类型；
- Command/事件契约与 TypeScript binding；
- 默认权限、可选权限和 scope schema；
- 每个平台实现或明确不支持策略；
- 初始化/卸载、资源清理和测试。

避免插件直接持有长期明文秘密或暴露任意系统能力。

## 6. 平台与故障验证

按顺序排查：依赖版本 -> feature/target 条件 -> Builder 注册 -> Capability -> API 名/参数 -> 运行时平台支持 -> 前端封装。

验证至少包括：

- `cargo check`/聚焦 Rust 测试；
- TypeScript 检查、前端测试和构建；
- 插件允许与拒绝路径；
- 初始化失败、权限拒绝、用户取消和资源释放；
- 各目标平台实际行为或 CI 证据；
- 页面变更使用 Codex 内置浏览器或 Control Chrome，系统能力使用真实 Tauri 运行时。


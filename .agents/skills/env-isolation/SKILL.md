---
name: env-isolation
description: |
  Tauri 开发/生产环境数据隔离技能，指导如何让 dev 模式与 prod 模式使用互不干扰的本地数据、日志级别和窗口标题。

  触发场景：
  - 开发调试时不想污染生产环境的真实数据（DB/配置/锁文件）
  - 需要区分 dev / prod 的日志详细程度
  - 需要在窗口标题标识当前运行的是开发版还是正式版
  - 需要为新 Tauri 项目规划环境隔离策略

  触发词：环境隔离、dev/prod、开发环境、生产环境、debug_assertions、数据隔离、dev-app.db、窗口标题、[DEV]、日志级别
---

# Tauri 开发/生产环境数据隔离

## 核心策略：同目录不同文件名

Tauri 的 `app_data_dir()` 由 `identifier`（如 `com.agilefr.tauri`）决定，dev 和 prod 拿到的是**同一个目录**。
若不做隔离，两者会**共享同一个数据库文件**——开发调试时的测试数据会直接污染用户的真实数据。

### 为什么不改 identifier？

| 方案 | 问题 |
|------|------|
| dev / prod 用不同 identifier | 打包/安装行为不一致、注册表/签名不一致、卸载残留 |
| dev / prod 共享目录 + **文件名前缀** ✅ | 只有一份 identifier、卸载行为一致、隔离清晰 |

本框架统一采用**第二种**。

---

## 实现三处隔离

### 1. 数据库文件名（必做）

```rust
// src-tauri/src/lib.rs 的 .setup()
let db_filename = if cfg!(debug_assertions) {
    "dev-app.db"   // 开发模式
} else {
    "app.db"       // 生产模式
};
let db_path = data_dir.join(db_filename);
```

**效果**：
- dev：`%APPDATA%/com.agilefr.tauri/dev-app.db`
- prod：`%APPDATA%/com.agilefr.tauri/app.db`

两份 DB 各有独立的 schema_version、各自的表数据，互不覆盖。

### 2. 日志级别（推荐）

```rust
.plugin(
    tauri_plugin_log::Builder::default()
        .level(if cfg!(debug_assertions) {
            log::LevelFilter::Info   // 开发：详细日志
        } else {
            log::LevelFilter::Warn   // 生产：只记警告及错误
        })
        .build(),
)
```

**效果**：生产版用户日志文件不会被海量 Info 日志刷爆。

### 3. 窗口标题前缀（推荐）

```rust
#[cfg(debug_assertions)]
if let Some(window) = app.get_webview_window("main") {
    if let Ok(current_title) = window.title() {
        let _ = window.set_title(&format!("{} [DEV]", current_title));
    }
}
```

**效果**：
- dev：`Agile Tauri [DEV]`
- prod：`Agile Tauri`（沿用 `tauri.conf.json` 的值）

> 肉眼一眼看出当前跑的是哪个版本，避免误把 dev 当正式版给别人演示。

---

## 扩展：多开实例 × 环境（进阶）

如果后续要做**多开实例**（同一台机器同时跑多个实例），要让 `实例 ID` 与 `环境` 两个维度正交：

```rust
// 锁文件前缀同样加上 dev-，避免开发实例 1 占用了生产实例 1 的锁
let lock_prefix = if cfg!(debug_assertions) { "dev-" } else { "" };
let lock_file = data_dir.join(format!("{}instance-{}.lock", lock_prefix, instance_id));

// 数据库按实例目录路由
let db_dir = if let Some(id) = instance_id {
    data_dir.join(format!("instance-{}", id))
} else {
    data_dir.clone()
};
```

这样 `dev-1`、`dev-2`、`1`、`2` 四个实例完全独立。

> 参考实现：`tauri-cc` 项目的 `src-tauri/src/lib.rs:550` 和 `:569`。

---

## 哪些数据不做隔离？

**共享的**（刻意设计，不要隔离）：
- 操作系统级凭证（如 `~/.claude`、`~/.codex` 这类 CLI 工具配置） — 让 dev 调试也能复用用户已有登录态
- `tauri-plugin-store` 的默认 store 文件（会和生产共用）— 如果这会造成污染，需要在 store 路径上也加 `dev-` 前缀

**必须隔离的**（本技能范围）：
- 应用自有 SQLite 数据库
- 应用自有锁文件、缓存文件
- 日志级别和输出量
- 窗口标题（非数据，但是避免误操作的关键）

---

## 检查清单

实现环境隔离时对照：

- [ ] DB 文件名加了 `dev-` 前缀？
- [ ] 日志级别按 `cfg!(debug_assertions)` 切换？
- [ ] 窗口标题在 dev 下加了 `[DEV]` 标识？
- [ ] 若使用 `tauri-plugin-store` 且会写敏感数据，store 文件名也已隔离？
- [ ] 多实例锁文件（如有）前缀与 DB 同步？

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| dev 和 prod 用不同 `identifier` | 同一 identifier + 文件名前缀 |
| 只隔离 DB、忘了锁文件 | 所有"同名互斥资源"都要加前缀 |
| 窗口标题写死在 conf.json 后又在 setup 里 `set_title` 覆盖 | 正确：prod 沿用 conf.json，dev 读当前 title 拼接 `[DEV]` |
| 生产日志级别留 `Info` | 生产应至少是 `Warn`，减少日志文件增长速度 |
| 把 dev/prod 判断散落在各 Service 中 | 集中在 `lib.rs::setup()` 里决定路径，下游 Service 只拿最终路径 |

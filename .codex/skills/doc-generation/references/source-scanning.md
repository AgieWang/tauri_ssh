# 开发者文档证据扫描

按文档类型选择对应章节。优先使用 `rg` / `rg --files`，并读取完整调用链，不能只统计文件名。

## Command/API 参考

至少核对：

1. `src-tauri/src/commands/**/*.rs` 中的 `#[tauri::command]` 函数签名、注释、注入状态和错误类型。
2. `src-tauri/src/commands/mod.rs` 的模块导出。
3. `src-tauri/src/lib.rs` 中 `generate_handler!` 的实际注册。
4. `src-tauri/src/models/**/*.rs` 中输入、输出结构和 serde 属性。
5. `src/types/**/*.ts` 中对应 TypeScript 类型。
6. `src/lib/api/**/*.ts` 中统一的 `invoke` 封装、参数名和返回类型。
7. `src/**/*.ts(x)` 中的真实调用点。
8. `src-tauri/capabilities/**/*.json` 中与插件或窗口相关的权限。

每个 Command 至少记录：

- Rust 函数名和文件位置。
- 业务描述和所属模块。
- 参数、Rust 类型、TypeScript 类型、可空性和命名转换。
- 返回类型和错误语义。
- State/AppHandle/Window 等注入依赖。
- 注册状态和前端封装入口。
- 真实调用页面或“暂未发现调用”。

不要把未注册的函数写成可用接口，不要把测试里的 `invoke` 写成生产调用点。

## 模块开发文档

围绕功能链路扫描，不按目录机械罗列：

```text
React 页面/组件
  → src/lib/api 封装
  → Tauri Command
  → Service
  → Database/系统能力
  → Model/事件/Capabilities
```

记录每层职责、入口、关键数据结构、错误传播和测试位置。若某层不存在，写明实际架构，不补造“标准层”。

## 数据库开发文档

扫描范围：

- `src-tauri/src/database/schema.rs` 和其他迁移文件。
- `PRAGMA user_version` 迁移顺序、条件和索引。
- Database/DAO 的 SQL、事务和行映射。
- Rust Model、serde 属性和 TypeScript 类型。
- 测试夹具、迁移测试和实际数据库路径配置。

需要准确 DDL 或数据格式时：

1. 读取项目脚本、Nacos 或实际连接配置，不能从样例猜连接。
2. 通过 Tauri SSH MCP 只读查询真实数据库。
3. 对比代码迁移与目标数据库现状。
4. 文档中标注查询环境、时间和无法确认的差异，不写明文凭据。

表结构至少记录字段名、存储类型、可空性、默认值、主键、唯一约束、索引、外键和迁移版本。字段说明以真实契约/DDL 为准，不以样例覆盖字段定义。

## IPC 与事件映射

Command 调用扫描之外，还要核对：

- Rust `emit` / `emit_to` / Channel 等发送端。
- React/TypeScript `listen` 等接收端。
- 事件名、Payload Rust/TS 类型和取消监听逻辑。
- 页面生命周期、并发和错误处理。

只有发送端或接收端的事件必须标注为“不完整链路”，不能描述成已闭环。

## 证据等级

| 等级 | 可以写入的措辞 |
|---|---|
| 当前代码已确认 | “代码中定义/注册/调用” |
| 测试已确认 | “对应测试覆盖……” |
| 运行时已确认 | “在指定环境验证……” |
| 仅静态推断 | “根据当前代码推断，尚未运行时验证” |
| 未确认 | 进入待确认项，不生成确定性结论 |


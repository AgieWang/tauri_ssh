---
name: json-serialization
description: |
  用于设计和验证 Rust serde、JSON 与 TypeScript 之间的序列化契约。

  触发场景：
  - 修改 Serialize、Deserialize 或 serde 属性
  - 设计 Tauri IPC 的复杂 JSON 参数、返回值或结构化错误
  - 处理 Option、枚举、日期、大整数、嵌套对象或自定义序列化
  - 排查 Rust 与 TypeScript 的实际 JSON 形状不一致

  不应触发：不改变 JSON 形状的普通业务字段映射、仅在数据库行与 Rust 模型之间赋值、普通类型重命名。

  触发词：serde、Serialize、Deserialize、serde_json、rename_all、tagged enum、JSON contract、custom serializer
---

# JSON 序列化与类型契约

## 适用边界

本 Skill 关注“线上的 JSON 实际长什么样”，而非所有类型转换。Tauri IPC 的端到端注册与 API 封装由 `api-development` 负责；本 Skill 负责 serde 属性、JSON 形状及 Rust/TypeScript 类型语义一致性。

```text
Rust model --serde--> JSON --Tauri IPC--> TypeScript type
```

## 契约不变量

1. 字段说明和实际序列化结果是契约，示例不能覆盖字段定义。
2. Rust、JSON、TypeScript 三端必须明确字段名、可空性、缺省与缺失的差异。
3. `Option<T>` 通常对应 `T | null`；若配合 `skip_serializing_if`，TypeScript 还必须允许属性缺失。
4. Rust `i64/u64` 可能超过 JavaScript 安全整数范围；标识符或大整数必须评估改用字符串，禁止无条件映射为 `number`。
5. 日期时间默认使用明确格式和时区的字符串；不能依赖浏览器隐式解析本地时间。
6. 枚举的大小写、tag/content 和未知值策略必须显式，并与 TypeScript union 对齐。
7. 敏感字段不得仅靠前端忽略；应在 Rust 返回模型中根本不序列化。

## 实施流程

1. 阅读真实 Rust model、Command 签名、`src/types/` 和 `src/lib/api/`；记录当前 JSON 形状。
2. 为请求和响应分别确定字段命名、必填、可空、默认、枚举与大整数策略。
3. 使用最少且明确的 serde 属性；避免同时在多层重复改名。
4. 同步 TypeScript 类型和解析逻辑，不使用 `any` 或不受控类型断言掩盖不一致。
5. 用真实序列化测试或 IPC 调用验证 JSON，不只依赖编译通过。

基础类型与精度边界见 [type-mapping.md](references/type-mapping.md)。字段重命名、默认值、Option、枚举和自定义序列化见 [serde-patterns.md](references/serde-patterns.md)。

## 关键选择

| 场景 | 处理方式 |
|---|---|
| 仅 Rust 返回前端 | `Serialize` |
| 前端传入 Rust | `Deserialize`，并做业务校验 |
| snake_case → camelCase | 在约定层统一使用 `rename_all`，同步 TS |
| 可空且总是存在 | `Option<T>` ↔ `T | null` |
| 可省略字段 | `skip_serializing_if` ↔ `field?: T` |
| 带数据枚举 | serde tagged enum ↔ discriminated union |
| 大整数/ID | 超出安全范围时以字符串传输 |
| 错误响应 | 项目 `CommandError` 结构，不临时返回任意字符串 |

## 不应触发示例

- “Mapper 把 status 列赋给 Rust 字段”且不跨 JSON 边界。
- “把页面变量 `userName` 政名为 `displayName`”但 API 契约不变。
- “设计 SQLite 表字段类型”——使用 `database-ops`。

## 最小测试矩阵

- 请求：完整字段、缺失字段、显式 `null`、错误类型和未知枚举。
- 响应：字段名、可省略属性、空集合、结构化错误和敏感字段缺失。
- 数值：0、负数、边界值、超过 JavaScript 安全整数的 ID。
- 时间：带时区、无时区、无效格式和跨日/夏令时边界。
- 兼容：旧客户端缺少新增字段、新客户端接收旧服务端响应。

## 与相关 Skill 的组合

- `api-development`：负责 Command 注册、参数名和 `src/lib/api/` 端到端调用。
- `database-ops`：负责 SQLite 列类型与行映射；不能用数据库类型替代 JSON 契约。
- `error-handler`：负责 `AppError`/`CommandError` 的传播与用户提示。

## 完成条件

- Rust/JSON/TypeScript 的字段名、可空性、缺省、枚举、日期和数值精度逐项对齐。
- 请求、响应和错误至少有聚焦序列化/反序列化测试。
- Tauri IPC 参数名和返回类型已通过真实调用或相关测试验证。
- `cargo fmt`、聚焦测试、前端类型检查和 `git diff --check` 通过。

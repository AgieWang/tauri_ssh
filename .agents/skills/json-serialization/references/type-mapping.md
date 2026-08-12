# Rust、JSON 与 TypeScript 类型映射

仅在定义 IPC JSON 契约或排查类型边界时读取。

| Rust | JSON | TypeScript | 注意 |
|---|---|---|---|
| `String`, `&str` | string | `string` | 明确编码与空字符串语义 |
| `bool` | boolean | `boolean` | SQLite 0/1 映射是另一层问题 |
| `i8..i32`, `u8..u32` | number | `number` | 仍需验证业务范围 |
| `i64`, `u64`, `usize` | number 或 string | `number` 或 `string` | 超过 `2^53-1` 必须避免 JS number 精度丢失 |
| `f32`, `f64` | number | `number` | JSON 不支持 NaN/Infinity |
| `Vec<T>` | array | `T[]` | 大数组评估 IPC 成本 |
| `Option<T>` | null/value | `T | null` | 若字段可省略还需 `?` |
| `HashMap<String,T>` | object | `Record<string,T>` | 非字符串 key 需自定义格式 |
| unit enum | string/number | literal union | 以 serde 配置为准 |
| data enum | object | discriminated union | 明确 tag/content |
| `Vec<u8>` | array/base64 | `number[]`/`string` | 大二进制不宜直接走 JSON |

## 可空、缺失和默认

以下三种契约不同：

```typescript
interface Example {
  presentNullable: string | null;
  optional?: string;
  optionalNullable?: string | null;
}
```

- `Option<T>` 默认常序列化为值或 `null`。
- `#[serde(skip_serializing_if = "Option::is_none")]` 会让属性缺失。
- `#[serde(default)]` 只影响反序列化缺失字段，不等于允许显式 `null`。

## 大整数

数据库 ID、字节数、时间戳和计数可能超过 JavaScript 安全整数：

```rust
#[derive(Serialize)]
struct RecordView {
    id: String,
}
```

TypeScript 对应 `id: string`。不要先转成 JS number 再转字符串，因为精度已经丢失。

## 日期时间

优先传输 ISO 8601/RFC 3339 且带时区，例如 `2026-08-01T10:30:00+08:00`。若沿用 SQLite 本地时间字符串，必须在契约中声明格式与时区，并使用显式解析逻辑。

## 二进制和大数据

- 小二进制可用 base64，但会增加体积。
- 大文件通过文件路径、流/Channel 或专门协议传输，避免巨型 JSON。
- 大数组分页或分批，记录 IPC 序列化成本。


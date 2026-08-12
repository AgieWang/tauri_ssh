# serde 模式参考

仅在处理字段命名、默认、Option、枚举、自定义格式或契约测试时读取。

## 字段命名

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInput {
    pub max_retries: u32,
    pub timeout_ms: u64,
    #[serde(rename = "type")]
    pub item_type: String,
}
```

对应 JSON：`maxRetries`、`timeoutMs`、`type`。是否使用 camelCase 必须以项目当前 IPC 契约为准；不要依赖“框架可能自动转换”的猜测。

## 默认与可省略

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub dark_mode: bool,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_port() -> u16 {
    8080
}
```

TypeScript 的 `description` 应为 `description?: string | null`（或根据反序列化输入契约进一步收窄）。

`#[serde(skip)]` 会同时跳过序列化和反序列化，不能随意用于必须从输入恢复的字段。只想单向跳过时使用对应的 `skip_serializing`/`skip_deserializing`。

## 枚举

简单枚举：

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    WaitingForApproval,
}
```

```typescript
type Status = "active" | "waiting_for_approval";
```

带数据枚举：

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Message {
    Text(String),
    Image { url: String, width: u32 },
}
```

```typescript
type Message =
  | { type: "text"; data: string }
  | { type: "image"; data: { url: string; width: number } };
```

对外部输入要定义未知枚举值行为：拒绝、映射 `unknown`，或保留原值。不要用不受控类型断言绕过。

## 自定义序列化

只有标准属性无法表达契约时才编写 `serialize_with`/`deserialize_with`。函数必须：

- 对合法、边界、缺失和错误输入有测试。
- 不吞掉解析错误。
- 明确时区、精度或规范化是否会改变原值。
- 避免把业务校验隐藏在序列化层；业务规则仍在 Service。

## 结构化错误

项目使用 `AppError` 作为内部错误、`CommandError` 作为可序列化 IPC 错误。新增错误码时同步前端解析类型，不把任意 `Debug` 字符串当稳定契约。

## 契约测试

```rust
#[test]
fn config_input_uses_camel_case() {
    let value = serde_json::to_value(ConfigInput {
        max_retries: 3,
        timeout_ms: 1000,
        item_type: "local".into(),
    })
    .expect("serialize ConfigInput");

    assert_eq!(value["maxRetries"], 3);
    assert_eq!(value["timeoutMs"], 1000);
    assert_eq!(value["type"], "local");
}
```

同时添加反序列化缺失字段、`null`、未知枚举、大整数和无效日期测试。


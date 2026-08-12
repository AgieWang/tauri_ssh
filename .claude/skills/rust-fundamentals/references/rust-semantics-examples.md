# Rust 语义最小示例

## 借用替代无必要 move

```rust
fn display_name(name: &str) {
    log::info!("name={name}");
}

let name = String::from("demo");
display_name(&name);
use_name_again(name);
```

## 缩短可变借用

```rust
let count = {
    let entry = values.entry(key).or_default();
    entry.push(value);
    entry.len()
};

// entry 的可变借用已结束，可以再次读取 values。
inspect(&values, count);
```

## 生命周期表达输入输出关系

```rust
fn choose<'a>(left: &'a str, right: &'a str) -> &'a str {
    if left.len() >= right.len() { left } else { right }
}
```

生命周期标注不延长 `left` 或 `right` 的寿命，只说明返回值不超过二者的共同有效期。

## Trait bound 放在真实需要的位置

```rust
fn render<T>(value: &T) -> String
where
    T: std::fmt::Display,
{
    value.to_string()
}
```

不要仅因某个内部实现需要 clone 就给整个公共类型添加 `Clone` bound。

## `.await` 前释放锁

```rust
let snapshot = {
    let guard = state
        .lock()
        .map_err(|error| AppError::State(error.to_string()))?;
    guard.clone_snapshot()
};

send_snapshot(snapshot).await?;
```

## 何时使用 Arc

```rust
let shared = std::sync::Arc::new(read_only_config);
let worker_config = std::sync::Arc::clone(&shared);
tokio::spawn(async move {
    run_worker(worker_config).await;
});
```

只有多个所有者确实跨任务存活时使用 `Arc`。只需转移给单个 task 时，直接 `move` 更清晰。

## 编译错误定位清单

- `E0382`：值已 move，确认谁应拥有。
- `E0499/E0502`：借用冲突，缩短作用域或拆分阶段。
- `E0515`：返回局部引用，改为返回拥有值或引用长期所有者。
- `E0277`：trait bound 不满足，检查具体类型、引用层级和 feature。
- Future 非 `Send`：查找跨 `.await` 的非 Send 值、锁守卫或引用。

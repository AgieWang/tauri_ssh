# 测试模式按需参考

## Rust 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_name() {
        let result = normalize_name("   ");
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }
}
```

## SQLite 隔离测试

```rust
fn setup_database() -> Result<Database, AppError> {
    Database::open_in_memory_for_test()
}

#[test]
fn transaction_rolls_back_on_conflict() -> Result<(), AppError> {
    let db = setup_database()?;
    // Arrange -> Act -> Assert，检查持久化结果而非只检查返回值。
    Ok(())
}
```

实际 helper 名称以仓库现有测试为准。迁移测试应覆盖旧 `user_version` 到当前版本，以及已有数据保留。

## Vitest API 封装测试

```typescript
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { itemApi } from "@/lib/api/item";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => vi.mocked(invoke).mockReset());

it("forwards the typed identifier", async () => {
  vi.mocked(invoke).mockResolvedValue({ id: 7, name: "demo" });

  await expect(itemApi.get(7)).resolves.toMatchObject({ id: 7 });
  expect(invoke).toHaveBeenCalledWith("get_item", { id: 7 });
});
```

## React 行为测试

- 通过角色、标签和可见文本查询元素，避免依赖 DOM 层级。
- 使用 `userEvent` 模拟真实交互。
- 等待可观察状态变化，不写固定延迟。
- 只 mock IPC/网络等外部边界，不 mock 被测组件内部实现。

## 常用命令

```bash
# Rust：先聚焦，再扩大
cd src-tauri
cargo test test_name -- --nocapture
cargo test module_name
cargo test

# 前端：以 package.json 的实际脚本为准
pnpm vitest run path/to/file.test.ts
pnpm test
npx tsc --noEmit
```

## 审查清单

- 测试名是否描述条件、动作和结果？
- 在错误实现上是否确实失败？
- 是否断言行为而非私有实现？
- fixture 是否最小且反映真实契约？
- 是否隔离 DB、文件、端口、时钟和全局状态？
- 是否存在裸 `unwrap()` 掩盖测试准备失败的上下文？
- 异步测试是否使用确定条件，而不是任意 sleep？
- 页面改动是否另行完成真实浏览器验收？

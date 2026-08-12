# 可复用工具与依赖参考

仅在已有跨模块复用证据，或需要评估通用 crate/npm 包时读取。

## Rust 候选能力

下列只是候选，不代表项目当前已安装或应固定版本。使用前检查 `Cargo.toml`、锁文件和当前官方文档。

| 能力 | 常见 crate | 评估重点 |
|---|---|---|
| 日期时间 | `chrono` / `time` | 时区、serde、体积 |
| UUID | `uuid` | 需要的版本类型与 feature |
| 正则 | `regex` | 编译复用、输入规模 |
| 目录遍历 | `walkdir` | 符号链接、权限、深度 |
| 哈希 | `sha2` | 安全用途与非安全校验区分 |
| Base64 | `base64` | 引擎/API 版本、大小开销 |
| HTTP | 项目既有客户端 | TLS、代理、超时、凭据 |

序列化、错误、异步、日志等核心能力优先复用项目已有 `serde`、`thiserror`、`tokio`、`log` 约定，不创建第二套抽象。

## Rust 纯工具函数

```rust
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
```

测试至少覆盖 0、1023、1024、单位边界和 `u64::MAX`。若产品要求 SI 单位（KB=1000）而非 IEC（KiB=1024），函数名和输出必须体现。

## TypeScript 工具函数

```typescript
export function debounce<TArgs extends unknown[]>(
  callback: (...args: TArgs) => void,
  delayMs: number,
): (...args: TArgs) => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (...args: TArgs) => {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => callback(...args), delayMs);
  };
}
```

真实 UI 场景还可能需要 `cancel`/`flush` 和组件卸载清理；不要把简化版本当完整通用库。

## 不要创建“万能 safeInvoke”

项目 IPC 已有统一 client/error 处理时，应扩展现有 `src/lib/api/`，而不是在 utils 新增返回 `[data, error]` 的第二套协议。否则结构化 `CommandError` 容易被降级为字符串并丢失错误码。

## 日期工具检查

- 输入是 UTC、带 offset 还是无时区本地字符串。
- 展示 locale 与存储格式分离。
- 无效日期返回 Result/明确占位，不静默变成当前时间。
- DST、闰日、月末和跨时区有测试。

## 字符串工具检查

- `trim`、大小写和 Unicode 规范化是否会改变业务标识。
- “长度”指字节、Unicode scalar 还是用户可见字符。
- 脱敏函数在短字符串、空值和多字节字符下不泄漏。
- 生成日志前先区分普通数据与凭据/敏感信息。

## 路径工具检查

路径组合使用 `Path`/`PathBuf` 或平台 path API。路径规范化和访问授权属于 `file-storage`/安全边界，本 Skill 不提供绕过 scope 的快捷函数。

## 依赖评估记录

在实现说明中至少记录：

- 当前重复实现和真实调用方。
- 自研与依赖方案的复杂度差异。
- 新依赖的 feature、传递依赖、许可证、维护状态和平台支持。
- 对 Rust 编译时间、前端 bundle 或安装包的影响。
- 移除或替换方案。


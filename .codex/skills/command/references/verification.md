# `/command` 验证与交付

## 1. 代码检查

- Command 名无冲突，`#[tauri::command]`、`pub`、模块导出、handler 注册完整。
- Command 仅做边界工作；Service/Database 职责正确，SQL 参数化。
- 没有 `unwrap()`、`panic!()`、`std::thread::sleep` 或同步长 IO。
- Rust 请求/响应具备必要 serde 派生；TypeScript 对齐 JSON 字段、null、枚举和大整数。
- API 位于 `src/lib/api/<domain>.ts`，通过 index 导出；组件不裸写 invoke。
- 错误使用当前结构化协议并由 `getErrorMessage` 处理。
- 依赖、插件、Builder 和 Capabilities 只增加实际需要的最小项。
- 外部输入、路径、URL、命令参数和日志完成安全审查。

## 2. 命令验证

根据改动选择并记录真实结果：

```bash
cd src-tauri && cargo fmt --check
cd src-tauri && cargo test <focused-test>
cd src-tauri && cargo check
cd src-tauri && cargo clippy
pnpm exec tsc --noEmit
pnpm test -- <focused-test>
pnpm build
git diff --check
```

不要为了交付声称运行了未执行的命令。仓库已有独立格式化脚本时优先使用当前脚本。

## 3. 运行时验证

- 真实 Tauri 运行时调用 Command，确认不存在 `Command not found`。
- 核对传入 payload 和返回 JSON，而非只看 TypeScript 编译。
- 验证成功、非法参数、依赖失败、空返回和重复调用。
- 有页面交互时，强制使用 Codex 内置浏览器或 Chrome；检查 loading、错误提示和控制台。
- 使用插件能力时验证 Capabilities 没有拒绝，且 scope 不过宽。

## 4. 交付格式

列出：

1. 实际修改/新增的文件及作用。
2. IPC 契约摘要（Command、入参、返回、错误）。
3. 依赖和权限变化；没有则明确没有。
4. 已运行的检查和结果。
5. 因环境限制未完成的运行时验证和具体原因。

不要默认继续执行 Git commit、push、发布或远程写入；这些需要用户明确授权。

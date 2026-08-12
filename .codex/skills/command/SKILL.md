---
name: command
description: |
  显式工作流：仅当用户输入 /command、$command，或明确要求“使用 command 脚手架生成单个 Tauri Command”时，编排 Rust IPC 入口及对应 TypeScript API 文件。

  触发场景：
  - 用户显式调用 /command 或 $command
  - 用户明确要求按 command 脚手架生成一个 Command
  - 用户明确要求使用 command 工作流生成单个 IPC 原型并列出全部注册文件

  不应触发：普通文字中的“命令”；一般 Command/IPC 实现；多个 Command 加完整页面的全栈功能；仅排查 Command 错误。

  强触发词：/command、$command、使用 command 脚手架、使用 command 脚手架生成单个 Tauri Command、Command 脚手架工作流
---

# `/command` 单 Command 脚手架

## 激活门禁

这是 `explicit-only` 工作流。没有显式信号时立即退出本工作流：

- 普通 IPC 契约改动使用 `api-development`。
- State/AppHandle/async/stream 高级实现使用 `tauri-commands`。
- 多 Command、完整页面和跨层模块使用显式 `dev` 工作流或领域 Skill 组合。

## 执行流程

1. 从用户描述和仓库证据提取：功能、模块、Command 名、输入、返回、同步/异步、注入对象、持久化和系统能力。只有缺失信息会实质改变契约或权限时才询问一次。
2. 读取当前相似实现：目标 `commands`、`services`、`models`、错误类型、`lib.rs`、前端 API/类型及依赖/Capabilities。
3. 搜索 Command 名和同类 API，避免重复定义或注册冲突。
4. 输出将修改的精确文件清单；不为了满足模板创建不需要的 Service、Database、Store 或权限文件。
5. 按当前仓库风格实现：
   - Model（按需）
   - Database（仅有 SQL 时）
   - Service（有业务逻辑或复用时）
   - Command 薄入口
   - Rust 模块导出与 handler 注册
   - TypeScript 类型与业务 API 封装
   - 调用示例或已有调用点
6. 格式化并运行聚焦检查；有 UI 调用时使用内置浏览器或 Chrome 验收。
7. 交付时列出实际修改、验证证据、未验证项和权限/依赖变化。

## 强制规则

- 先读参考代码再写，命名、错误类型和目录以当前仓库为准。
- Command 只处理 IPC、输入校验、Service 调用和错误转换；SQL 只能在 Database 层。
- 可失败操作不得 `unwrap()`/`panic!()`；异步等待不得用 `std::thread::sleep`。
- Command 必须 `#[tauri::command]`、公开导出并加入 `generate_handler![]`。
- 前端不得在组件裸写 `invoke()`；封装到 `src/lib/api/<domain>.ts` 并从 index 导出。
- Rust/TS 字段名、null、枚举、时间和大整数按实际 serde JSON 对齐，不宣称自动字段转换。
- 外部 HTTP 由 Rust 代理并进行安全校验；插件 API 必须检查 Builder 和 Capabilities。
- 新依赖只在确有必要时添加并说明原因；不扩大未使用权限。
- 所有代码格式化；最终运行 `git diff --check`。

## 按需读取 References

- 需要确定输入和文件范围：读取 [脚手架输入与文件决策](references/scaffolding-inputs.md)。
- 需要具体 Rust/TS 骨架：读取 [Command 模板](references/command-templates.md)。
- 准备交付或检查遗漏：读取 [验证与交付](references/verification.md)。
- 复杂注入、进度、子进程：转用 `tauri-commands` references，不在本入口重复加载。

## 正反向示例

| 用户请求 | 是否激活 |
|---|---|
| “/command 新增 `read_config`” | 是 |
| “请用 $command 搭一个进度 Command 原型” | 是 |
| “新增一个 Tauri Command” | 否，默认由 `api-development`/`tauri-commands` 处理 |
| “运行 cargo 命令” | 否 |
| “完整做一个用户管理模块” | 否，显式 `/dev` 才进入全栈编排 |

用户显式激活只授权实现其请求范围，不自动授权安装依赖、扩大权限、提交、推送或发布。

## 完成条件

- [ ] 明确是单 Command 范围，没有误扩为完整模块。
- [ ] 只创建实际需要的文件，分层和契约与现有代码一致。
- [ ] Rust 定义、注册、TS 类型、API 封装和调用点闭环。
- [ ] 输入、错误、权限、平台差异和敏感数据已审查。
- [ ] 格式化、聚焦测试/检查、必要的浏览器验收和 diff check 通过。

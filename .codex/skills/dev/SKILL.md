---
name: dev
description: |
  显式工作流：仅当用户输入 /dev、$dev，或明确要求“使用 dev 全栈工作流”开发完整 Tauri 功能模块时，编排 Rust 三层、IPC、React UI、状态、数据库和权限。

  触发场景：
  - 用户显式调用 /dev 或 $dev
  - 用户明确要求按 dev 工作流实现完整跨层功能
  - 用户明确要求使用 dev 工作流一次交付多个 Command、完整页面、状态和持久化

  不应触发：普通“开发/新功能/页面开发”措辞；单个 Command；仅前端页面；仅修 Bug；仅输出方案。

  强触发词：/dev、$dev、使用 dev 全栈工作流、dev 完整功能模块、全栈脚手架工作流
---

# `/dev` 全栈实现编排

## 激活门禁

这是 `explicit-only` 工作流。未出现显式信号时退出，不因“开发”“新功能”“模块”“页面”等宽泛词自动接管普通编码任务。

- 单个端到端 IPC：`api-development`。
- 单个显式脚手架：`command`。
- React 页面：`ui-frontend`。
- 数据库、权限、安全、文件、事件等继续加载对应领域 Skill，不能由 `/dev` 替代。

## 执行阶段

1. **界定目标**：从需求提取用户流程、数据来源、系统能力、持久化、页面、失败行为和验收标准。只有影响架构或外部权限的关键歧义才询问。
2. **防重复**：用 `rg` 检查现有 Command、Service、Database、API、Types、Store、Page 和路由；已有能力优先扩展。
3. **读取模式**：读取每个涉及层的一处相似实现、错误类型、`lib.rs`、Capabilities、依赖和前端布局/API 客户端。
4. **设计契约与文件图**：先列 Rust/TS 类型、Command 契约、数据/状态归属、精确文件清单和验证项；不空造层。
5. **按依赖顺序实现**：Models/Database -> Services -> Commands/注册 -> Types/API -> Store/Page/Router -> Capabilities/依赖。
6. **分层验证**：每完成一层运行聚焦检查；最终格式化、编译/测试、`git diff --check`，页面强制浏览器验收。
7. **交付证据**：说明实际文件、契约、数据库/权限变化、检查结果、运行时证据和未完成风险。

## 不可削弱的准确性规则

- 先读真实参考代码，目录、错误类型、迁移和状态模式不按旧模板猜测。
- Rust 保持 Commands -> Services -> Database；Command 不写业务/SQL，Service 不绕过 Database。
- 所有可失败操作传播错误，禁止 `unwrap()`/`panic!()`；异步路径禁止阻塞 sleep。
- Rust/TypeScript 按实际 serde JSON 对齐，结构体字段不会无条件自动 camelCase。
- 前端 API 统一封装，组件不裸写 invoke；外部 HTTP 默认经 Rust 代理并执行安全校验。
- 全局状态仅在确有跨组件共享时使用 Zustand；局部状态留在组件。
- 插件必须检查 Rust 注册、前端依赖和最小 Capabilities；未使用权限不得加入。
- SQLite 改动遵循当前 `PRAGMA user_version` 迁移机制，SQL 参数化；不得回退到旧式 `init_database()` 假设。
- 删除要确认，表单要验证，页面必须呈现 loading/empty/error/success 状态。
- 不自动执行 Git commit/push、发布、远程写入、生产数据修改或凭据操作。

## 按需读取 References

- 规划跨层数据流与文件：读取 [全栈计划与文件矩阵](references/fullstack-plan.md)。
- 编码或审查全栈强制规则：读取 [生成规则与验证](references/generation-rules.md)。
- IPC 契约细节读取 `api-development`；页面模式与浏览器验收读取 `ui-frontend`。
- 数据库/Capabilities/文件/事件/通知等只在实际涉及时读取对应 Skill。

## 正反向示例

| 用户请求 | 是否激活 |
|---|---|
| “/dev 完整开发知识库管理模块” | 是 |
| “用 $dev 做 CRUD、页面和 SQLite” | 是 |
| “帮我开发一个 React 页面” | 否，使用 `ui-frontend` |
| “新增一个 Command” | 否，使用 IPC 领域 Skill |
| “先设计这个模块方案” | 否，处于方案阶段不得进入实现 |

显式激活意味着使用完整编排，不代表每一层都必须创建；文件仍由真实需求决定。发现已有实现时以扩展/修复为主，不能为了脚手架另起平行模块。

## 交付证据

按层列出修改原因和核心内容，给出格式化、聚焦测试、编译/构建、浏览器及真实数据/权限验证的实际结果；未完成项注明原因和风险。

构建、HTTP 200、截图或测试通过都是中间证据；只有与需求验收标准一致的真实行为才能作为功能完成结论。

## 完成条件

- [ ] 用户流程、契约、数据归属和文件图与实现一致。
- [ ] 只修改实际需要的层，无重复能力和架构绕行。
- [ ] Rust 注册、TS API、路由/状态、权限/依赖全部闭环。
- [ ] 格式化、聚焦测试、Rust/TS 检查、构建和 diff check 通过。
- [ ] 页面由 Codex 内置浏览器或 Chrome 验收，控制台和失败态无回归。

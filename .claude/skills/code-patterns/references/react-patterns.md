# React 与 TypeScript 模式

## 组件职责

- 页面负责路由级编排和数据加载。
- 可复用组件接收明确 props，不直接知道所有全局状态。
- 自定义 Hook 封装可复用副作用和异步状态，但不隐藏关键业务错误。
- 大组件按独立变化原因拆分，不按任意行数机械切割。

使用函数组件与 Hooks。Effect 明确依赖、取消和 cleanup；Tauri listener 必须解除订阅。

## API client 与类型

当前仓库优先采用：

```text
src/lib/api/client.ts     # 统一 invoke / 错误解析
src/lib/api/<domain>.ts   # 分域 API
src/lib/api/index.ts      # 统一导出
src/types/<domain>.ts     # 分域契约
src/types/index.ts        # 类型统一导出
```

- 组件不裸写 `invoke`。
- Rust 字段、枚举、Option、日期和错误结构与 TypeScript 对齐。
- `unknown` 在边界处缩窄，禁止用 `any` 绕过契约。
- Re-export Hub 保持稳定公共入口，但避免循环依赖。

## 状态模式

- 局部交互：`useState`/`useReducer`。
- 共享 UI/设置状态：按职责拆分 Zustand store，并由 `src/store/index.ts` 统一导出。
- 后端持久数据：Rust + SQLite，通过 API 查询；不要把 store 当数据库。
- 服务端/后端加载状态可封装为领域 Hook，处理 loading/error/retry/竞态。

选择器尽量窄，避免订阅整个 store 导致无关重渲染。

## UI 与样式

- Ant Design 用于表单、表格、弹窗和交互组件。
- TailwindCSS 用于布局和间距。
- CSS Variables 与 Ant Design token 表达颜色、阴影、圆角等设计令牌。
- 不硬编码主题色，不使用与项目 `data-theme` 机制冲突的独立 dark 方案。
- 保留语义 HTML、键盘、焦点、label 和 aria 可访问性。

## 抽取判断

抽 Hook/组件/API 的前提是调用者共享相同语义和生命周期。不要：

- 把不同业务动作塞进一个布尔参数众多的万能组件。
- 为一处简单请求创建过深的 Hook/Service 包装。
- 在 Store 中复制后端实体并产生双向同步源。
- 用通用错误 toast 吞掉可操作错误码。

## 验证

```bash
npx tsc --noEmit
pnpm vitest run <focused-target>
pnpm build
git diff --check
```

页面与样式变更必须在 Codex 内置浏览器或 Control Chrome 验证关键交互、控制台、加载/空/错误态和布局。

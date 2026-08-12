# Zustand 与 Rust AppState 模式

仅在实现跨组件/跨页面状态或 Rust 进程共享资源时读取。

## Zustand 按领域拆分

```text
src/store/
├── app.ts
├── settings.ts
└── index.ts
```

```typescript
import { create } from "zustand";

type ThemeMode = "light" | "dark" | "system";

interface AppStore {
  theme: ThemeMode;
  sidebarCollapsed: boolean;
  setTheme: (theme: ThemeMode) => void;
  toggleSidebar: () => void;
}

export const useAppStore = create<AppStore>((set) => ({
  theme: "system",
  sidebarCollapsed: false,
  setTheme: (theme) => set({ theme }),
  toggleSidebar: () =>
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}));
```

外部统一从 `@/store` 导入，组件订阅最小切片：

```typescript
const theme = useAppStore((state) => state.theme);
const setTheme = useAppStore((state) => state.setTheme);
```

不要在组件中 `useAppStore()` 订阅整个对象，除非确实需要每个字段。

## action 与副作用

setter 应保持语义直接：`setTheme(theme)` 只存储 theme。不要在 setter 中偷偷折叠侧栏、跳转路由、刷新数据或写磁盘；否则 URL 同步、事件恢复等其他调用路径也会触发副作用。

复杂 action 如果确实要执行请求或持久化，应使用能表达副作用的名称，并明确 loading/error/竞态：

```typescript
interface SettingsStore {
  saving: boolean;
  saveThemePreference: (theme: ThemeMode) => Promise<void>;
}
```

异步请求可使用 request id、AbortController 或版本号避免旧响应覆盖新状态。

## React Context

Context 适合有天然 Provider 边界、更新频率低的依赖。高频全局 UI 状态优先沿用项目已有 Zustand。不要为同一状态同时维护 Context 和 Zustand 两个真相源。

## Rust AppState

```rust
pub struct AppState {
    pub database: Database,
    pub runtime: Mutex<RuntimeState>,
}

#[tauri::command]
pub fn read_runtime(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeView, CommandError> {
    RuntimeService::read(&state.runtime).map_err(Into::into)
}
```

- 使用 `.manage(AppState { ... })` 注册一次。
- 共享可变状态只保存必要数据，不把大对象无界累积在内存。
- 获取锁后尽快完成纯内存操作并释放；网络/文件/数据库异步操作不在锁内等待。
- 锁错误映射为项目错误类型，禁止 `unwrap()`。
- Rust 权威状态通过 Command 或事件同步给前端，避免前端自行猜测。

## 测试

- selector 只因目标切片变化而更新。
- action 对边界输入的状态转换正确。
- 异步旧响应不会覆盖新值。
- Rust 锁错误和并发更新可诊断。
- 卸载、切页、重新连接后不会更新已失效组件。


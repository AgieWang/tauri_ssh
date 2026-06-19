---
name: store-management
description: |
  Tauri 状态管理技能,覆盖 React 前端状态和 Rust 后端状态管理。

  触发场景:
  - 需要管理前端组件间共享状态
  - 需要在 Rust 后端管理应用状态
  - 需要持久化存储(tauri-plugin-store)
  - 需要设计全局状态架构

  触发词: 状态管理、state、store、全局状态、共享状态、Zustand、Context、持久化
---

# Tauri 状态管理

## 双层状态架构

```
┌──────────────────────────────────────────┐
│  前端状态 (React)                          │
│  ├── 组件内: useState                      │
│  ├── 全局状态: Zustand (src/store/*.ts)     │
│  ├── API 封装: src/lib/api/index.ts        │
│  └── Hooks: src/hooks/useCommand.ts        │
├──────────────────────────────────────────┤
│  IPC 桥接 (invoke / listen)                │
├──────────────────────────────────────────┤
│  后端状态 (Rust - 三层架构)                 │
│  ├── 运行时: tauri::State<AppState>        │
│  │   (定义于 src-tauri/src/state.rs)       │
│  ├── 持久化: tauri-plugin-store            │
│  └── 数据库: rusqlite (SQLite)             │
│      (src-tauri/src/database/)             │
└──────────────────────────────────────────┘
```

### 关键文件位置

| 状态类型 | 文件 |
|---------|------|
| Rust AppState 定义 | `src-tauri/src/state.rs` |
| Database 结构体 | `src-tauri/src/database/mod.rs` |
| Schema 迁移 | `src-tauri/src/database/schema.rs` |
| 前端 Zustand Store (UI 状态) | `src/store/app.ts` |
| 前端 Zustand Store (设置状态) | `src/store/settings.ts` |
| 前端 Store 统一出口 | `src/store/index.ts` |
| API 类型安全封装 | `src/lib/api/index.ts` |
| invoke Hook 封装 | `src/hooks/useCommand.ts` |

---

## React 前端状态

### 方案 1: useState(组件内状态)

```tsx
function Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(c => c + 1)}>Count: {count}</button>;
}
```

### 方案 2: React Context(跨组件共享)

```tsx
import { createContext, useContext, useState, ReactNode } from "react";

interface AppContextType {
  theme: "light" | "dark";
  setTheme: (theme: "light" | "dark") => void;
  user: string | null;
  setUser: (user: string | null) => void;
}

const AppContext = createContext<AppContextType | null>(null);

function AppProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [user, setUser] = useState<string | null>(null);

  return (
    <AppContext.Provider value={{ theme, setTheme, user, setUser }}>
      {children}
    </AppContext.Provider>
  );
}

function useApp() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}
```

### 方案 3: Zustand(轻量全局状态,推荐)

```bash
pnpm add zustand
```

#### Store 按职责拆分

Store 不再集中在单文件，而是按职责拆分为独立模块，通过 `index.ts` 统一导出：

```
src/store/
├── app.ts        # UI 状态（theme、sidebarCollapsed）
├── settings.ts   # 设置状态（language、closeBehavior）
└── index.ts      # Re-export Hub（统一导出入口）
```

**`src/store/app.ts`** — UI 状态:

```tsx
import { create } from "zustand";

interface AppStore {
  theme: "light" | "dark";
  setTheme: (theme: "light" | "dark") => void;
  sidebarCollapsed: boolean;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
}

export const useAppStore = create<AppStore>((set) => ({
  theme: "light",
  setTheme: (theme) => set({ theme }),
  sidebarCollapsed: false,
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}));
```

**`src/store/settings.ts`** — 设置状态:

```tsx
import { create } from "zustand";

interface SettingsStore {
  language: string;
  setLanguage: (lang: string) => void;
  closeBehavior: "quit" | "tray";
  setCloseBehavior: (behavior: "quit" | "tray") => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  language: "zh-CN",
  setLanguage: (language) => set({ language }),
  closeBehavior: "quit",
  setCloseBehavior: (closeBehavior) => set({ closeBehavior }),
}));
```

**`src/store/index.ts`** — Re-export Hub:

```tsx
// 统一导出所有 store，外部始终从 @/store 导入
export { useAppStore } from "./app";
export { useSettingsStore } from "./settings";
```

#### 使用方式

```tsx
// 从统一入口导入（推荐）
import { useAppStore, useSettingsStore } from "@/store";

function MyComponent() {
  const theme = useAppStore((s) => s.theme);
  const language = useSettingsStore((s) => s.language);
  return <div>Theme: {theme}, Lang: {language}</div>;
}
```

---

## Rust 后端状态

### tauri::State<T>(运行时状态)

```rust
use std::sync::Mutex;

struct AppState {
    counter: Mutex<u32>,
    config: Mutex<AppConfig>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            counter: Mutex::new(0),
            config: Mutex::new(AppConfig::default()),
        }
    }
}

// 注册
tauri::Builder::default()
    .manage(AppState::default())

// 使用
#[tauri::command]
fn increment(state: tauri::State<'_, AppState>) -> Result<u32, String> {
    let mut counter = state.counter.lock().map_err(|e| e.to_string())?;
    *counter += 1;
    Ok(*counter)
}
```

### tauri-plugin-store(键值持久化)

```bash
# Cargo.toml
tauri-plugin-store = "2"
# package.json
pnpm add @tauri-apps/plugin-store
```

```rust
// Rust 注册
tauri::Builder::default()
    .plugin(tauri_plugin_store::Builder::default().build())
```

```typescript
// TypeScript 使用
import { Store } from "@tauri-apps/plugin-store";

const store = await Store.load("settings.json");
await store.set("theme", "dark");
const theme = await store.get<string>("theme");
await store.save();  // 持久化到磁盘
```

---

## 主题配置规范

主题切换涉及多层协作：Zustand 管理运行时状态、`tauri-plugin-store` 持久化到磁盘、Ant Design `ConfigProvider` 应用主题、CSS Variables 提供设计令牌。

### 数据流

```
应用启动
  │
  ▼
tauri-plugin-store 读取磁盘偏好 (settings.json)
  │
  ▼
设置 Zustand store (useAppStore.setTheme)
  │
  ▼
App.tsx useEffect → resolveTheme(theme) → resolved = "dark" | "light"
  │
  ├──► document.documentElement.setAttribute("data-theme", resolved)
  │       → :root[data-theme] CSS 变量切换（variables.css）
  │
  └──► ConfigProvider theme={getAntdTheme(resolved)}
          → Ant Design 组件自动响应（antdTheme.ts）

用户切换主题时:
  useAppStore.toggleTheme() → resolved 更新 → data-theme + ConfigProvider 同步响应
```

### 关键代码位置

| 职责 | 文件 |
|------|------|
| 主题运行时状态 | `src/store/app.ts` (`useAppStore`) |
| 主题持久化 | `tauri-plugin-store` → `settings.json` |
| 主题应用 | `src/App.tsx` (`ConfigProvider theme={}`) |
| 主题 token 配置 | `src/theme/antdTheme.ts` |
| CSS 设计令牌 | `src/styles/variables.css` |

### 初始化模式

```tsx
// App.tsx 中初始化主题
import { Store } from "@tauri-apps/plugin-store";
import { useAppStore } from "@/store";
import { getAntdTheme } from "@/theme/antdTheme";

function App() {
  const theme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);

  // 启动时从 plugin-store 加载持久化偏好
  useEffect(() => {
    (async () => {
      const store = await Store.load("settings.json");
      const saved = await store.get<string>("theme");
      if (saved === "dark" || saved === "light") {
        setTheme(saved);
      }
    })();
  }, []);

  return (
    <ConfigProvider theme={getAntdTheme(theme)}>
      {/* ... */}
    </ConfigProvider>
  );
}
```

### 切换时持久化

```tsx
async function handleThemeToggle() {
  const next = theme === "light" ? "dark" : "light";
  setTheme(next); // 更新 Zustand → ConfigProvider 立即响应
  const store = await Store.load("settings.json");
  await store.set("theme", next);
  await store.save(); // 持久化到磁盘
}
```

---

## 选型建议

| 场景 | 推荐方案 | 文件位置 |
|------|---------|---------|
| 组件内简单状态 | `useState` | 组件内 |
| 全局 UI 状态(主题/侧边栏) | Zustand | `src/store/app.ts` |
| 全局设置状态(语言/关闭行为) | Zustand | `src/store/settings.ts` |
| 需要持久化的设置 | tauri-plugin-store + Zustand | plugin-store 持久化 → Zustand 运行时 |
| 业务数据(配置等) | Rust State + Command (三层架构) | `src-tauri/src/services/` |
| 大量结构化数据 | rusqlite (SQLite) | `src-tauri/src/database/` |
| API 调用封装 | 类型安全 invoke 封装 | `src/lib/api/index.ts` |

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 所有状态放前端 | 持久化和业务数据放 Rust 侧 |
| 过度使用全局状态 | 优先 useState,必要时才升级 |
| Mutex 不处理 PoisonError | 使用 `.map_err()` 处理 |
| 不序列化就存 store | 确保数据可 JSON 序列化 |
| setter 里塞副作用（「同值翻转」「写完自动刷新」等） | setter 纯存值；副作用放调用处。否则从其他路径（URL 同步 / 事件）触发 setXxx 时会意外触发副作用导致状态漂移（典型：点菜单同路由触发 set → 副作用折叠面板） |

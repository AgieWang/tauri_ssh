---
name: ui-frontend
description: |
  React 前端 UI 组件开发技能,指导 Tauri 桌面应用的界面开发。

  触发场景:
  - 需要开发 React 页面或组件
  - 需要选择和使用 UI 组件库
  - 需要处理表单、表格、弹窗等常见 UI
  - 需要实现响应式布局

  触发词: UI、组件、页面、前端、界面、表单、表格、弹窗、布局、样式、React
---

# React 前端 UI 开发

## 概述

Tauri 桌面应用的前端运行在系统 WebView 中,使用 React 19 + TypeScript 5.8 + Ant Design + TailwindCSS 4 开发。与 Web 应用的主要区别是:窗口大小可控、无需考虑 SEO、可调用系统 API。

### 前端项目结构

```
src/
├── components/
│   ├── layout/
│   │   ├── AppLayout.tsx        # 主布局（Ant Design Layout）
│   │   └── Sidebar.tsx          # 侧边栏导航
│   └── ui/
│       └── ErrorBoundary.tsx    # 错误边界
├── hooks/
│   └── useCommand.ts           # invoke 封装
├── lib/
│   └── api/
│       ├── client.ts           # invoke 封装 + getErrorMessage 工具
│       ├── config.ts           # 配置相关 API
│       ├── system.ts           # 系统相关 API
│       └── index.ts            # 统一导出
├── pages/
│   ├── home/index.tsx           # 首页
│   ├── settings/index.tsx       # 设置页
│   └── about/index.tsx          # 关于页
├── store/
│   ├── app.ts                  # 应用状态（主题/侧边栏）
│   ├── settings.ts             # 设置状态（持久化 ↔ tauri-plugin-store）
│   └── index.ts                # 统一导出
├── styles/
│   ├── variables.css           # CSS 设计令牌（双主题颜色/间距/阴影）
│   └── global.css              # TailwindCSS + 全局样式
├── theme/
│   └── antdTheme.ts            # Ant Design 主题配置（dark/light）
├── types/
│   ├── config.ts               # 配置相关类型
│   ├── system.ts               # 系统相关类型
│   └── index.ts                # 统一导出
├── App.tsx                      # 根组件（ConfigProvider + Router）
├── Router.tsx                   # React Router 配置
└── main.tsx                     # 入口
```

### 当前技术栈

| 技术 | 用途 |
|------|------|
| **Ant Design** | UI 组件库（Layout/Form/Table 等） |
| **TailwindCSS 4** | 原子化 CSS 样式 |
| **React Router** | 客户端路由 |
| **Zustand** | 全局状态管理 |

---

## UI 组件库（已选用 Ant Design）

项目已集成 **Ant Design** 作为主要 UI 组件库,配合 **TailwindCSS 4** 做原子化样式补充。

| 关键组件 | 用途 | 参考文件 |
|---------|------|---------|
| `Layout / Sider / Content` | 主布局 | `src/components/layout/AppLayout.tsx` |
| `Menu` | 侧边栏导航 | `src/components/layout/Sidebar.tsx` |
| `ConfigProvider` | 全局主题配置 | `src/App.tsx` |
| `Form / Input / Select` | 表单 | 各页面组件 |
| `Table` | 数据表格 | 各页面组件 |
| `Modal / message` | 弹窗/消息 | 各页面组件 |

---

## 组件开发模式

### 基础组件模板

```tsx
import { useState } from "react";
import { message } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage } from "@/lib/api";

interface Props {
  title: string;
}

function FeaturePage({ title }: Props) {
  const [data, setData] = useState<DataType[]>([]);
  const [loading, setLoading] = useState(false);

  async function loadData() {
    setLoading(true);
    try {
      const result = await invoke<DataType[]>("get_data");
      setData(result);
    } catch (e) {
      message.error(getErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  if (loading) return <div className="loading">加载中...</div>;

  return (
    <div className="page">
      <h1>{title}</h1>
      <div className="content">
        {data.map(item => (
          <div key={item.id}>{item.name}</div>
        ))}
      </div>
    </div>
  );
}

export default FeaturePage;
```

> **错误处理规范**: 所有 catch 块统一使用 `getErrorMessage(error)` 提取错误信息，禁止 `` `加载失败: ${error}` `` 模板字符串拼接。
> ```tsx
> import { getErrorMessage } from "@/lib/api";
> // catch (e) { message.error(getErrorMessage(e)); }
> ```

### 表单组件

```tsx
import { useState, FormEvent } from "react";
import { message } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage } from "@/lib/api";

interface FormData {
  name: string;
  email: string;
  description: string;
}

function CreateForm() {
  const [form, setForm] = useState<FormData>({
    name: "", email: "", description: ""
  });

  function handleChange(field: keyof FormData, value: string) {
    setForm(prev => ({ ...prev, [field]: value }));
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await invoke("create_item", { input: form });
      setForm({ name: "", email: "", description: "" });
    } catch (e) {
      message.error(getErrorMessage(e));
    }
  }

  return (
    <form onSubmit={handleSubmit}>
      <label>
        名称
        <input value={form.name} onChange={e => handleChange("name", e.target.value)} required />
      </label>
      <label>
        邮箱
        <input type="email" value={form.email} onChange={e => handleChange("email", e.target.value)} />
      </label>
      <label>
        描述
        <textarea value={form.description} onChange={e => handleChange("description", e.target.value)} />
      </label>
      <button type="submit">保存</button>
    </form>
  );
}
```

### 列表 + CRUD 页面

```tsx
function ItemList() {
  const [items, setItems] = useState<Item[]>([]);
  const [editing, setEditing] = useState<Item | null>(null);

  useEffect(() => { loadItems(); }, []);

  async function loadItems() {
    const list = await invoke<Item[]>("list_items");
    setItems(list);
  }

  async function deleteItem(id: number) {
    if (!confirm("确认删除?")) return;
    await invoke("delete_item", { id });
    await loadItems();
  }

  return (
    <div>
      <table>
        <thead>
          <tr><th>ID</th><th>名称</th><th>操作</th></tr>
        </thead>
        <tbody>
          {items.map(item => (
            <tr key={item.id}>
              <td>{item.id}</td>
              <td>{item.name}</td>
              <td>
                <button onClick={() => setEditing(item)}>编辑</button>
                <button onClick={() => deleteItem(item.id)}>删除</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

---

## 设置页规范

设置页使用 **Drawer + Tabs + Form** 模式，从右侧滑入，无需路由切换。

### 设计原则

| 原则 | 说明 |
|------|------|
| **Drawer 而非路由** | 设置面板用 `Drawer` 从右侧滑入，不占用路由 |
| **Tabs 分类** | 使用 `Tabs` 对设置项分类（通用、外观、关于等） |
| **自动保存** | `onValuesChange` 触发自动保存，无需手动点击保存按钮 |
| **持久化** | Zustand store 与 tauri-plugin-store 双向同步 |

### SettingsDrawer 标准模板

```tsx
import { Drawer, Tabs, Form, Switch, Select, message } from "antd";
import { useAppStore } from "@/store";
import { useSettingsStore } from "@/store";
import { getErrorMessage } from "@/lib/api";

interface SettingsDrawerProps {
  open: boolean;
  onClose: () => void;
}

export default function SettingsDrawer({ open, onClose }: SettingsDrawerProps) {
  const [form] = Form.useForm();
  const settings = useSettingsStore();

  // Tabs 切换时重置 form 值（防止脏数据）
  function handleTabChange() {
    form.resetFields();
  }

  // 自动保存：任意值变化时触发
  async function handleValuesChange(changed: Record<string, unknown>) {
    try {
      await settings.update(changed);
    } catch (e) {
      message.error(getErrorMessage(e));
    }
  }

  const tabItems = [
    {
      key: "general",
      label: "通用",
      children: (
        <Form
          form={form}
          layout="vertical"
          initialValues={settings.general}
          onValuesChange={handleValuesChange}
        >
          <Form.Item name="autoStart" label="开机启动" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="language" label="语言">
            <Select options={[
              { label: "简体中文", value: "zh-CN" },
              { label: "English", value: "en-US" },
            ]} />
          </Form.Item>
        </Form>
      ),
    },
    {
      key: "appearance",
      label: "外观",
      children: (
        <Form
          form={form}
          layout="vertical"
          initialValues={settings.appearance}
          onValuesChange={handleValuesChange}
        >
          <Form.Item name="theme" label="主题">
            <Select options={[
              { label: "跟随系统", value: "system" },
              { label: "浅色", value: "light" },
              { label: "深色", value: "dark" },
            ]} />
          </Form.Item>
          <Form.Item name="fontSize" label="字号">
            <Select options={[
              { label: "小", value: "small" },
              { label: "中", value: "medium" },
              { label: "大", value: "large" },
            ]} />
          </Form.Item>
        </Form>
      ),
    },
    {
      key: "about",
      label: "关于",
      children: <div className="text-sm text-gray-500">版本信息、开源协议等</div>,
    },
  ];

  return (
    <Drawer
      title="设置"
      placement="right"
      width={480}
      open={open}
      onClose={onClose}
      destroyOnClose
      maskClosable={false}
    >
      <Tabs items={tabItems} onChange={handleTabChange} />
    </Drawer>
  );
}
```

### 持久化同步模式（Zustand + tauri-plugin-store）

```tsx
// src/store/settings.ts
import { create } from "zustand";
import { load } from "@tauri-apps/plugin-store";

interface SettingsState {
  general: { autoStart: boolean; language: string };
  appearance: { theme: string; fontSize: string };
  load: () => Promise<void>;
  update: (changed: Record<string, unknown>) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  general: { autoStart: false, language: "zh-CN" },
  appearance: { theme: "system", fontSize: "medium" },

  load: async () => {
    const store = await load("settings.json", { autoSave: false });
    const general = await store.get<SettingsState["general"]>("general");
    const appearance = await store.get<SettingsState["appearance"]>("appearance");
    if (general) set({ general });
    if (appearance) set({ appearance });
  },

  update: async (changed) => {
    const store = await load("settings.json", { autoSave: false });
    // 合并到当前状态
    const state = get();
    const newState = { ...state, ...changed };
    set(newState);
    // 持久化到 tauri-plugin-store
    for (const [key, value] of Object.entries(changed)) {
      await store.set(key, value);
    }
    await store.save();
  },
}));
```

> **要点**: 应用启动时调用 `useSettingsStore.getState().load()` 从磁盘加载设置。

---

## 弹窗/面板遮罩关闭交互规范

桌面应用的 Modal/Drawer 要按"用户失去的成本 vs 得到的便利"决定是否允许点击遮罩关闭。

### 三类场景与对应设置

| 类型 | 示例 | `maskClosable` | 理由 |
|------|------|----------------|------|
| **编辑/表单/操作类** | 新建/编辑表单、设置、激活码输入、多步操作弹窗 | `false` | 含用户输入，误点遮罩会丢数据 |
| **纯查看/信息展示类** | 关于/版本信息、只读详情弹窗 | `true`（默认） | 纯浏览，快速关闭体验更好 |
| **命令浮层类** | 命令面板、搜索、快速选择 | `true`（默认） | VSCode 风格，失焦即关 |

### Ant Design 组件配置

```tsx
// ✅ 编辑/表单类 Modal：必须 maskClosable={false}
<Modal
  open={visible}
  title="编辑项目"
  onCancel={onClose}
  onOk={handleSubmit}
  maskClosable={false}   // ← 关键
>
  <Form>
    <Form.Item name="name" label="名称"><Input /></Form.Item>
  </Form>
</Modal>

// ✅ 纯信息 Modal：保持默认即可
<Modal open={visible} title="关于" onCancel={onClose}>
  <div>版本 1.0.0</div>
</Modal>

// ✅ Drawer 同理（设置 Drawer 包含表单时必须禁用遮罩关闭）
<Drawer
  title="设置"
  open={open}
  onClose={onClose}
  maskClosable={false}   // ← 表单类 Drawer 必加
>
  <Form onValuesChange={handleValuesChange}>...</Form>
</Drawer>
```

> **Ant Design v6 新写法**：`mask={{ closable: false }}` 等价于 `maskClosable={false}`，两者任选其一，**不要同时写**。

### 判断口诀

- **里面有 input/textarea/form 或多步操作** → `maskClosable={false}`
- **只是查看一段信息、没有输入** → 保持默认 `true`
- **命令面板/搜索/快速选择** → 保持默认 `true`（VSCode 惯例）

### 配套建议

- 所有含表单的弹窗都应同时支持 **Esc 关闭** 和 **右上角 X 按钮**（Ant Design 默认已提供）
- 关闭前可弹二次确认提示（有 unsaved 变更时）

---

## 桌面应用 UI 注意事项

| 注意事项 | 说明 |
|---------|------|
| 窗口大小 | 默认 800x600,可在 tauri.conf.json 配置 |
| 无滚动条 | 桌面应用通常避免页面级滚动 |
| 系统菜单 | 可通过 Tauri Menu API 实现原生菜单 |
| 拖拽区域 | 使用 `data-tauri-drag-region` 创建可拖拽标题栏 |
| 快捷键 | 可通过 Tauri 全局快捷键 API 注册 |
| 主题系统 | 通过 `data-theme` 属性 + CSS 变量切换暗色/亮色，详见 `theme-system` 技能 |

### 样式选择规则

| 场景 | 方案 | 示例 |
|------|------|------|
| Ant Design 组件内 | `token.*`（`useToken()`） | `token.colorBgContainer` |
| 自定义组件颜色 | CSS 变量 `var(--xxx)` | `background: var(--bg-secondary)` |
| 布局/间距 | TailwindCSS 原子类 | `className="flex gap-4 p-6"` |
| 边框颜色 | CSS 变量 | `border: 1px solid var(--border)` |
| TailwindCSS 引用变量 | arbitrary values | `bg-[var(--bg-hover)]` |

### 关键主题文件

| 文件 | 职责 |
|------|------|
| `src/styles/variables.css` | 设计令牌（颜色/间距/阴影/圆角/字体） |
| `src/theme/antdTheme.ts` | Ant Design 主题配置（`getAntdTheme(resolved)`） |
| `src/store/app.ts` | 主题状态管理（dark/light/system 三态） |

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 使用 `window.alert()` | 使用自定义弹窗组件或 Tauri dialog 插件 |
| 使用 `window.open()` | 使用 Tauri 窗口 API 或 opener 插件 |
| 硬编码颜色值 `#1a1a1c` | 使用 `var(--bg-primary)` 或 `token.colorBgLayout` |
| 使用 TailwindCSS `dark:` 前缀 | 使用 CSS 变量 `var(--xxx)` 或 `data-theme` 选择器 |
| 使用绝对像素布局 | 使用 flexbox/grid 响应式布局 |
| 组件过大不拆分 | 按功能拆分为 < 200 行的小组件 |
| `` message.error(`加载失败: ${error}`) `` | `message.error(getErrorMessage(error))` + `import { getErrorMessage } from "@/lib/api"` |
| 设置页用独立路由 | 使用 `Drawer` 从右侧滑入，无需路由切换 |
| 所有 API/类型/store 写在单文件 | 按模块拆分（`api/config.ts`、`store/settings.ts`、`types/system.ts`） |
| 表单 Modal/Drawer 允许点击遮罩关闭 | 必须加 `maskClosable={false}`，防止误点丢失输入 |
| `<iframe src={convertFileSrc(abs)}>` 预览本地 PDF/HTML，内嵌在 Modal | 部分老 WebView2 / 严格 CSP 下 iframe 加载 asset: 协议被拦成「已阻止此内容」；各机器行为不一 | Modal title 右侧固定加一个「用系统应用打开」小按钮调 `openPath(abs)`，作为跨环境兜底；不要依赖 iframe 的 onerror（拦截不会触发） |

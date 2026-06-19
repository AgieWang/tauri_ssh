---
name: theme-system
description: |
  Tauri 桌面应用��题系统技能，覆盖设计令牌、CSS 变量、Ant Design 主题定制、暗色/亮色切换机制。

  触发场景：
  - 需要添加或修改 CSS 设计令牌（颜色/间距/阴影等��
  - 需要定制 Ant Design 主题（品牌色/组件样式）
  - 需要适配暗色/亮色双主题
  - 需要新组件正确响应主题切换
  - 需要理解主题系统的数据流和架构

  触发词：主题、theme、暗色、亮色、dark、light、CSS变量、设计令牌、design token、配色、antdTheme、variables.css
---

# 主题系统指南

## 概述

本项目采用 **CSS 变量 + Ant Design 主题 + data-theme 属性** 三层主题架构，支持暗色/亮色/跟随系统三种模式。CSS 变量是 source of truth，Ant Design token 与之同步。

## 架构数据流

```
Zustand (useAppStore.theme)
  │
  ▼
App.tsx useEffect → resolveTheme("system" → "dark"|"light")
  │
  ├──► document.documentElement.setAttribute("data-theme", resolved)
  │       → :root[data-theme="dark/light"] CSS 变量切换
  │       → 所有使用 var(--xxx) 的元素自动响应
  │
  └──► ConfigProvider theme={getAntdTheme(resolved)}
          → Ant Design 组件自动响应
```

## 关键文件

| 文件 | 职责 |
|------|------|
| `src/styles/variables.css` | 设计令牌（CSS 变量，双主题定义） |
| `src/styles/global.css` | 全局样式（引入 variables.css + TailwindCSS） |
| `src/theme/antdTheme.ts` | Ant Design 主题配置（darkTheme/lightTheme） |
| `src/App.tsx` | 主题应用入口（data-theme + ConfigProvider） |
| `src/store/app.ts` | 主题状态管理（Zustand） |

## 设计令牌体系

### 语义化颜色

| 变量 | 暗色值 | 亮色值 | 用途 |
|------|--------|--------|------|
| `--bg-primary` | #1a1a1c | #ffffff | 主背景 |
| `--bg-secondary` | #232325 | #f5f5f7 | 面板/侧边栏背景 |
| `--bg-tertiary` | #2a2a2d | #ebebef | 悬浮面板/弹窗 |
| `--bg-hover` | #333336 | #e0e0e4 | 悬停状态 |
| `--bg-active` | #2a4a82 | #d6e4f8 | 选中/激活状态 |
| `--text-primary` | #dcdcde | #1a1a2e | 主文本 |
| `--text-secondary` | #8a8a8d | #5c5c6e | 次要文本 |
| `--text-muted` | #707074 | #9898a6 | 禁用/占位文本 |
| `--accent` | #5090f0 | #2563eb | 品牌色/主操作 |
| `--danger` | #f0554a | #dc2626 | 危险/错误 |
| `--success` | #5ec26a | #16a34a | 成功 |
| `--warning` | #e8c44a | #d97706 | 警告 |
| `--border` | #353538 | #d9d9de | 主边框 |

### 通用令牌（不随主题切换）

| 类别 | 变量 | 值 |
|------|------|------|
| 间距 | `--spacing-xs` ~ `--spacing-2xl` | 4px ~ 24px |
| 圆角 | `--radius-sm` ~ `--radius-xl` | 3px ~ 12px |
| 过渡 | `--transition-fast/normal/slow` | 0.1s/0.15s/0.2s |
| 字体 | `--font-ui`, `--font-mono` | 系统字体栈 |
| 字号 | `--font-size` | 13px |
| 布局 | `--sidebar-width`, `--header-height` | 220px, 48px |

## 使用规范

### 何时用 CSS 变量 vs Ant Design token

| 场景 | 方案 | 示例 |
|------|------|------|
| Ant Design 组件内部 | `token.*`（通过 `useToken()`） | `token.colorBgContainer` |
| 自定义组件/非 Ant Design | `var(--xxx)` CSS 变量 | `background: var(--bg-secondary)` |
| Layout 布局边框 | `var(--border)` | `border: 1px solid var(--border)` |
| TailwindCSS 类中引用 | CSS 变量（通过 arbitrary values） | `bg-[var(--bg-hover)]` |
| 全局样式/伪元素 | CSS 变量 | `::selection { background: var(--selection-bg) }` |

### 新增设计令牌的步骤

1. 在 `src/styles/variables.css` 的 `:root[data-theme="dark"]` 和 `light` 中分别添加变量
2. 如果是 Ant Design 也需要用到的颜色，同步到 `src/theme/antdTheme.ts`
3. 在组件中使用 `var(--新变量名)`

### 新组件适配主题的规范

```tsx
// ✅ 正确：使用 CSS 变量
<div style={{ background: "var(--bg-secondary)", color: "var(--text-primary)" }}>

// ✅ 正确：在 Ant Design 组件上下文中用 token
const { token } = antdTheme.useToken();
<Card style={{ background: token.colorBgContainer }}>

// ❌ 错误：硬编码颜色
<div style={{ background: "#1a1a1c", color: "#dcdcde" }}>

// ❌ 错误：使用 TailwindCSS 硬编码暗色类
<div className="bg-gray-900 dark:bg-white">
```

## Ant Design 主题配置

`src/theme/antdTheme.ts` 导出：

- `darkTheme: ThemeConfig` — 暗色主题完整配置
- `lightTheme: ThemeConfig` — 亮色主题完整配置
- `getAntdTheme(resolved): ThemeConfig` — 根据 resolved 获取主题

### 组件级覆盖

已覆盖的组件：Button、Input、Select、Modal、Table、Tooltip。

新增组件覆盖时，在 `sharedComponents` 中添加（两个主题共享），或在各自的 `components` 中添加（主题差异化）。

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 硬编码颜色值 `#1a1a1c` | 使用 `var(--bg-primary)` |
| 只改 CSS 变量不改 antdTheme | 两处同步修改 |
| 在 variables.css 只加暗色忘了亮色 | dark/light 两个 block 都要加 |
| 用 TailwindCSS `dark:` 前缀 | 用 `var(--xxx)` 或 `data-theme` 选择器 |
| 在 antdTheme 中用默认色 | 使用与 CSS 变量一致的自定义色 |
| body 背景不跟随主题 | `body { background: var(--bg-primary) }` 已在 global.css |

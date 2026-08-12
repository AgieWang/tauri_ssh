---
name: theme-system
description: |
  用于维护 tauri_ssh 的主题系统，包括暗色/亮色/跟随系统、CSS Variables 设计令牌、Ant Design ThemeConfig 与主题状态同步。

  触发场景：
  - 新增或修改暗色/亮色设计令牌
  - 调整 `variables.css` 与 `antdTheme.ts` 的颜色或组件 token
  - 修复主题切换、跟随系统、主题持久化或组件不响应主题的问题

  不应触发：普通页面布局、局部 CSS 微调、表单/表格/弹窗开发、品牌文案或原生窗口外观。

  强触发词：暗色亮色主题、CSS Variables 设计令牌、antdTheme、ThemeConfig、data-theme、主题切换、跟随系统主题
---

# 主题系统

## 职责边界

本 Skill 只处理主题架构、设计令牌和暗亮模式。普通页面/组件样式使用 `ui-frontend`；窗口标题栏、原生窗口外观使用 `tauri-window-management`。

当前主题链路应以真实代码为准，通常为：

```text
Zustand theme（light/dark/system）
  -> 解析 resolved theme
  -> document.documentElement[data-theme]
  -> variables.css 的语义 CSS Variables
  -> getAntdTheme(resolved) / ConfigProvider
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src/styles/variables.css` | 暗色/亮色语义令牌 |
| `src/styles/global.css` | 全局消费与 Tailwind 入口 |
| `src/theme/antdTheme.ts` | Ant Design token/algorithm/组件覆盖 |
| `src/App.tsx` | `data-theme` 和 `ConfigProvider` 应用入口 |
| 当前应用 Store | theme、resolvedTheme 与持久化 |

先读取这些文件和当前调用方，不能按旧文件名或固定 Store 结构猜测。

## 修改流程

1. 明确修改的是语义令牌、Ant Design token、主题状态还是解析逻辑。
2. 搜索令牌的定义和全部使用方，避免改名后残留或建立重复变量。
3. 新增颜色令牌时，在 dark/light 两个主题块都定义，并使用语义名称而非页面名称。
4. Ant Design 组件需要同一语义时，同步更新 `ThemeConfig`；不要假设 CSS Variable 会自动进入 antd token。
5. 页面和自定义组件消费 `var(--token)`；Ant Design 上下文使用 `theme.useToken()` 或当前 ThemeConfig。
6. 若修改 `system` 模式，验证系统偏好变化监听、持久化值和 resolved theme，不把 resolved 值覆盖用户选择。
7. 格式化、类型检查、构建，并用内置浏览器或 Chrome 验证暗色、亮色和跟随系统。

## 设计令牌规则

- 颜色使用语义名：`--bg-primary`、`--text-secondary`、`--border`、`--accent`、`--danger` 等。
- 间距、圆角、字体、阴影和过渡可作为跨主题共享令牌；只有真实差异才分别定义。
- 自定义组件不硬编码十六进制主题色；布局/间距可用 Tailwind，主题颜色使用变量。
- 不使用 Tailwind `dark:` 另建并行主题体系；以 `data-theme` + CSS Variables 为唯一页面主题来源。
- Ant Design 颜色与 CSS 语义令牌保持视觉一致；若配置中必须写静态值，评审时逐项对照两个主题。
- 保证文本、禁用态、边框、hover/active/focus 的对比度，不只检查默认态。

## 使用示例

```tsx
// 自定义组件：消费语义变量
<section
  className="flex gap-4 p-4 bg-[var(--bg-secondary)]"
  style={{ color: "var(--text-primary)" }}
/>
```

```tsx
// Ant Design 上下文：消费 token
const { token } = theme.useToken();
<Card style={{ background: token.colorBgContainer }} />
```

新增令牌：

```css
:root[data-theme="dark"] {
  --panel-emphasis: #2a2a2d;
}

:root[data-theme="light"] {
  --panel-emphasis: #f0f0f3;
}
```

## 常见错误

| 错误 | 修正 |
|---|---|
| 只给 dark 定义变量 | dark/light 同步定义并分别验收 |
| 改 CSS 不改 antdTheme | 核对相同语义的 Ant Design token |
| 组件硬编码 `#1a1a1c` | 使用语义 CSS Variable/token |
| 使用 `dark:` 维护第二套主题 | 使用 `data-theme` 和 CSS Variables |
| 把 resolved dark 写回用户 theme | 保存 `system`，resolved 只用于渲染 |
| 只验收截图 | 实际切换主题并检查交互态和控制台 |

## 完成条件

- [ ] 文件为 UTF-8 无 BOM，中文和 Frontmatter 无乱码。
- [ ] 令牌语义清晰，dark/light 定义完整，无重复或失效引用。
- [ ] CSS Variables、Ant Design token 和主题状态链路一致。
- [ ] 格式化、类型检查、构建和 `git diff --check` 通过。
- [ ] 内置浏览器或 Chrome 验证 light、dark、system 和 focus/hover/disabled 状态。

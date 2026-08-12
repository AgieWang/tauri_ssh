# 设置与桌面布局模式

## 目录

1. 模式选择
2. Drawer/Tabs/Form
3. Zustand 与持久化
4. 桌面布局

## 1. 模式选择

设置页优先遵循当前产品入口：已有 Drawer 就扩展 Drawer，已有独立路由就扩展路由。只有新增设计时再比较：

| 模式 | 适合 |
|---|---|
| Drawer + Tabs | 快速设置、保持当前上下文 |
| 独立路由 | 设置复杂、需要深链接/大空间 |
| Modal | 少量一次性选项 |

不要把“设置必须 Drawer”当成全局硬规则。

## 2. Drawer / Tabs / Form

```tsx
<Drawer
  title="设置"
  placement="right"
  width={480}
  open={open}
  onClose={onClose}
  destroyOnHidden
  mask={{ closable: false }}
>
  <Tabs
    items={[
      { key: "general", label: "通用", children: <GeneralSettings /> },
      { key: "appearance", label: "外观", children: <AppearanceSettings /> },
      { key: "about", label: "关于", children: <AboutSettings /> },
    ]}
  />
</Drawer>
```

自动保存必须有：防抖、提交状态、失败回滚/提示、并发顺序和关闭前行为。设置安全敏感或影响连接时，优先显式保存。

```tsx
<Form
  form={form}
  layout="vertical"
  initialValues={settings}
  onValuesChange={scheduleSave}
>
  <Form.Item name="theme" label="主题">
    <Select options={[
      { label: "跟随系统", value: "system" },
      { label: "浅色", value: "light" },
      { label: "深色", value: "dark" },
    ]} />
  </Form.Item>
</Form>
```

Tab 切换是否 reset 取决于草稿策略；不能无条件 `resetFields()` 丢弃用户未保存输入。

## 3. Zustand 与持久化

- 组件可独立拥有的设置草稿留在组件。
- 跨布局消费者（主题、侧栏、全局偏好）可放 Zustand。
- 持久化层沿用当前 tauri-plugin-store/API，不在 Store 内散布裸 invoke。
- load 区分“尚未加载”和默认值；避免默认值抢先覆盖磁盘数据。
- update 只合并数据字段，不把 action 函数展开写回持久化。

示意：

```typescript
interface SettingsState {
  loaded: boolean;
  appearance: AppearanceSettings;
  load: () => Promise<void>;
  updateAppearance: (patch: Partial<AppearanceSettings>) => Promise<void>;
}
```

加载和保存失败必须可见；不要静默吞错。敏感配置不得存入普通 plugin-store，应转 Safe Credentials。

## 4. 桌面布局

- 使用 Flex/Grid/Tailwind 组织区域，明确哪个容器滚动。
- 在最小窗口、常用窗口和较大窗口验证；长文本、表格和 Drawer 不溢出。
- 拖拽标题栏仅在 `data-tauri-drag-region` 安全区域使用，按钮/输入不能误拖拽。
- 原生菜单、快捷键、托盘和多窗口分别使用对应 Tauri Skill，不在 UI 组件里模拟系统行为。
- 主题颜色使用 CSS Variables/antd token；间距和布局使用 Tailwind。
- 页面级滚动是否允许由内容决定，禁止用 `overflow: hidden` 隐藏不可达内容。

### 主题关键路径

- `src/styles/variables.css`：语义设计令牌；
- `src/theme/antdTheme.ts`：Ant Design 映射；
- `src/App.tsx`：ConfigProvider 与 data-theme；
- 当前 Zustand store：theme/resolvedTheme 状态。

具体主题修改转 `theme-system`，普通页面只消费这些 token。

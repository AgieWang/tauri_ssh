# react-i18next 实现参考

仅在接入 i18n、增加 locale、维护资源或实现语言切换时读取。

## 初始化

先检查项目是否已有 i18n 实例；只有不存在时才新增。典型配置：

```typescript
import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import enUS from "./locales/en-US.json";
import zhCN from "./locales/zh-CN.json";

void i18n.use(initReactI18next).init({
  resources: {
    "zh-CN": { translation: zhCN },
    "en-US": { translation: enUS },
  },
  lng: "zh-CN",
  fallbackLng: "zh-CN",
  interpolation: { escapeValue: false },
});

export default i18n;
```

如果语言由 Zustand/plugin-store 恢复，不要同时让浏览器 detector 和应用设置争夺真相源。明确优先级：用户显式设置 > 系统/浏览器语言 > fallback。

入口文件在渲染 App 前导入初始化模块：

```typescript
import "@/i18n";
```

## 资源组织

```json
{
  "app": {
    "title": "Tauri SSH"
  },
  "actions": {
    "save": "保存",
    "cancel": "取消"
  },
  "tasks": {
    "completed": "已完成 {{count}} 个任务"
  }
}
```

- key 使用稳定英文点分路径。
- 按领域/namespace 控制资源体积。
- 所有 locale 的 key 和插值变量保持一致。
- 不复制整句只改一个标点的重复 key；也不把语义不同的文案强行共用。

## React 使用

```tsx
import { Select } from "antd";
import { useTranslation } from "react-i18next";

export function LanguageSelector() {
  const { i18n, t } = useTranslation();
  return (
    <Select
      aria-label={t("settings.language")}
      value={i18n.resolvedLanguage ?? "zh-CN"}
      options={[
        { value: "zh-CN", label: "简体中文" },
        { value: "en-US", label: "English" },
      ]}
      onChange={(locale) => void i18n.changeLanguage(locale)}
    />
  );
}
```

用户选择需持久化时使用项目 store/action，保存失败给出页面反馈。

## 日期、数字与货币

```typescript
const formatted = new Intl.DateTimeFormat(locale, {
  dateStyle: "medium",
  timeStyle: "short",
  timeZone,
}).format(date);

const amount = new Intl.NumberFormat(locale, {
  style: "currency",
  currency,
}).format(value);
```

时区和币种来自明确业务字段/设置，不根据语言随意推断。

## 复数与插值

使用 i18next 的 `count` 和当前版本支持的 plural key 规范。不得用 `count > 1 ? ...` 在组件中拼接不同语言语法。插值参数名在所有 locale 中一致。

## Ant Design 联动

语言切换通常还要同步 `ConfigProvider locale`。读取当前 Ant Design 版本的 locale 导出，并验证 DatePicker、Pagination、Table 空状态和 Modal 默认按钮文案。

## 测试与验收

- 自动比较所有 locale 的 key 和插值参数。
- 验证缺失 key、fallback 和无效 locale。
- 验证刷新/重启后的语言恢复。
- 用内置浏览器或 Chrome 切换至少两种语言，检查长文本、菜单、表格、弹窗和日期组件。
- 原生菜单/通知不由 React 自动翻译，需要分别核验。


---
name: i18n-development
description: |
  用于维护 React 应用的 i18n、locale 资源、语言切换和本地化格式。

  触发场景：
  - 配置 i18next/react-i18next 或新增 locale 资源
  - 实现语言切换、fallback、复数或插值
  - 本地化日期、数字、货币或多语言布局

  不应触发：只修改一处中文业务文案、普通翻译请求、代码注释翻译、没有 locale 资源变化的 UI 修改。

  触发词：i18n、react-i18next、i18next、locale、语言切换、翻译资源、l10n、fallbackLng
---

# React 国际化开发

## 适用边界

本 Skill 处理国际化机制和资源一致性。普通中文文案修正不自动触发；需要同步 locale 或改变本地化行为时才使用。

## 实施流程

1. 检查现有初始化、locale、namespace、持久化和 Ant Design locale；不重复建第二套体系。
2. 明确支持的 locale、默认语言、fallback、资源加载方式和缺失 key 行为。
3. 使用稳定的英文点分 key，按领域组织；所有支持语言保持 key 集合一致。
4. 插值、复数、日期、数字和货币使用 i18next/`Intl` 能力，不用字符串拼接模拟语法。
5. 语言偏好由 `store-management` 负责运行时与持久化，避免同步循环。
6. 验证即时切换、重启恢复、fallback、长文本和组件 locale。

初始化、资源组织、组件调用和格式化示例见 [react-i18next-patterns.md](references/react-i18next-patterns.md)。

## 关键规则

- React 组件通过 `useTranslation()` 获取 `t`，非组件代码使用项目既有 i18n 实例。
- 翻译 key 不使用展示文案本身，避免文案变化导致 key 不稳定。
- 不在翻译值中拼接未转义 HTML；确需富文本时使用受控组件映射。
- 变量名、复数 count 和日期时区必须在所有语言下验证。
- 不把后端错误原文直接当翻译 key；使用稳定错误码映射本地文案。
- 新增语言时同步 Ant Design、日期库和非 React 文案。

## 不应触发示例

- “把按钮文字从保存改成确定”，且项目没有要求更新多语言资源。
- “把一段中文翻译成英文”。
- “修复 CSS 让中文不换行”但不涉及多语言布局。

## 完成条件

- 支持语言的 key 集合、插值参数和 namespace 一致，缺失 key 有检查。
- 切换、fallback、持久化与格式化行为通过测试。
- 页面变更已使用内置浏览器或 Chrome 验证至少两种 locale 和长文本。
- 前端格式化、类型检查、聚焦测试与 `git diff --check` 通过。

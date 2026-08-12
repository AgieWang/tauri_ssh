---
name: collaborating-with-gemini
description: |
  用于用户明确要求调用 Gemini CLI、委托外部 Gemini 或进行多模型对比的场景；本 Skill 为 explicit-only，普通 UI、CSS、React 和代码审查任务不自动触发。

  触发场景：
  - 用户明确要求“使用 Gemini CLI”生成前端或视觉候选方案
  - 用户明确要求把同一问题委托给外部 Gemini 做多模型对比
  - 用户明确要求继续已有 Gemini CLI 会话

  触发词：Gemini CLI、调用 Gemini、委托 Gemini、多模型对比、gemini协同、$collaborating-with-gemini
---

# 与 Gemini CLI 协同

## 激活边界

本技能是 `explicit-only` 外部协作能力。只有用户明确指定 Gemini CLI、外部 Gemini 或本技能名时才激活。

以下情况不触发：

- 普通 React 页面、UI、CSS、响应式布局或样式修改，改用 `ui-frontend`。
- 普通代码审查、Bug 排查或组件设计。
- 用户没有提出外部委托，仅要求当前代理实现前端任务。

不得把“UI设计”“CSS”“样式”等普通前端词作为调用外部模型的充分条件。

## 强制安全规则

1. 显式只读：当前桥接脚本的 `--sandbox` 默认值为关闭，每次调用都必须显式传入 `--sandbox`。
2. 不允许 Gemini 直接修改工作区；若用户明确要求写入，也必须先说明权限影响，并继续遵守当前任务范围和仓库并发规则。
3. 不把 API Key、Token、密码、私钥、Cookie、连接串或凭据文件内容放入 Prompt、日志或回复。
4. 不得通过 Prompt 的文件引用扩大读取范围；引用前确认文件不含密钥、生产数据或无关敏感信息。
5. Gemini 输出只作为候选原型或审查意见；必须由当前代理核对项目技术栈、现有组件模式、类型、安全和业务契约。
6. 页面改动最终仍须使用 Codex 内置浏览器或 Control Chrome 验收，外部模型截图或描述不能替代真实页面验证。

## 执行流程

1. 确认用户明确要求 Gemini 协作，并确定目标、输入文件和期望输出。
2. 检查桥接脚本和 Gemini CLI 是否可用；缺失时报告，不自动安装或修改认证配置。
3. 显式传入 `--sandbox` 发起只读调用，优先请求候选设计、结构化审查或 Unified Diff。
4. 对输出做本地复核：Ant Design、TailwindCSS、React 19、类型、无障碍、现有设计令牌和响应式行为。
5. 如需落地，由当前代理实现经复核的最小变更；不要原样接受“完整组件代码”。
6. 执行前端格式化、类型检查、测试、构建和强制浏览器验收。

## 按需读取

需要桥接参数、`@file`、多轮会话或故障排查时，读取：

- [bridge-usage.md](references/bridge-usage.md)

仅咨询是否应调用 Gemini 时，不读取该参考文件。

## 完成条件

- 外部调用由用户明确要求，且输入范围不包含敏感信息。
- 调用保持只读，没有扩大目录或工具权限。
- 外部输出已经按当前项目代码与 UI 规范复核。
- 落地页面已完成本地检查和真实浏览器验收。

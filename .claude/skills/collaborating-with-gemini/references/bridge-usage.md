# Gemini CLI 桥接用法

仅在 `SKILL.md` 的显式触发和安全边界均满足时读取本文件。

## 调用入口

在项目根目录执行只读调用。当前脚本默认不启用沙箱，因此 `--sandbox` 不得省略：

```bash
python .codex/skills/collaborating-with-gemini/scripts/gemini_bridge.py \
  --cd . \
  --sandbox \
  --PROMPT "Propose a UI candidate using the existing project design system."
```

脚本返回 JSON，常用字段为 `success`、`SESSION_ID`、`agent_messages` 和 `error`。

## 参数

| 参数 | 必填 | 默认值 | 用途与限制 |
|---|---|---|---|
| `--PROMPT` | 是 | — | 任务说明；不得包含凭据或生产数据 |
| `--cd` | 是 | — | 已授权工作区根目录 |
| `--sandbox` | 否 | 关闭 | 安全规则要求每次显式传入 |
| `--SESSION_ID` | 否 | 无 | 会话索引或 `latest` |
| `--return-all-messages` | 否 | `False` | 仅故障排查时使用，并检查敏感输出 |
| `--model` | 否 | 无 | 仅用户明确指定模型时传入 |

## 常见只读模式

### UI 候选方案

```bash
python .codex/skills/collaborating-with-gemini/scripts/gemini_bridge.py \
  --cd . \
  --sandbox \
  --PROMPT "Design a responsive React component candidate using Ant Design 5, TailwindCSS 4 and existing CSS variables. Do not modify files."
```

### 引用项目文件

仅引用已检查且不含敏感信息的文件：

```bash
python .codex/skills/collaborating-with-gemini/scripts/gemini_bridge.py \
  --cd . \
  --sandbox \
  --PROMPT "Review @src/pages/example/index.tsx for accessibility and responsive behavior. Return findings only."
```

### Unified Diff 候选

```bash
python .codex/skills/collaborating-with-gemini/scripts/gemini_bridge.py \
  --cd . \
  --sandbox \
  --PROMPT "Generate a minimal Unified Diff candidate for the requested UI change. Do not modify files. OUTPUT: Unified Diff Patch ONLY."
```

### 多轮会话

```bash
python .codex/skills/collaborating-with-gemini/scripts/gemini_bridge.py \
  --cd . \
  --sandbox \
  --SESSION_ID "latest" \
  --PROMPT "Revise the proposal to follow the supplied local design tokens and accessibility findings."
```

## 输出复核清单

- 组件、Hook、API 和设计令牌在当前仓库中存在。
- 使用 React 19 函数组件、Ant Design、TailwindCSS 和 `@/` 路径别名。
- 没有裸写 `invoke()`、引入 Node.js API 或绕过 Rust Command 代理外部请求。
- TypeScript 没有引入 `any`，错误态、加载态、空态和无障碍行为完整。
- Diff 不覆盖其他会话未提交改动。
- 页面已由当前代理使用内置浏览器或 Control Chrome 验收。

## 故障排查

| 现象 | 处理 |
|---|---|
| `gemini: command not found` | 报告缺少 CLI；不要自动全局安装 |
| 认证不可用 | 提示用户自行安全登录；不要读取或回显 API Key |
| 会话恢复失败 | 核对索引或 `latest`，必要时开启新的只读会话 |
| 输出截断 | 谨慎使用 `--return-all-messages`，先确认不会暴露敏感信息 |
| 脚本超时 | 报告超时并保留现有工作，不转用 YOLO 或扩大权限 |
| Windows 路径错误 | 使用正斜杠或经过核对的绝对路径 |
| 候选代码不符技术栈 | 丢弃或由当前代理按本项目模式重写 |

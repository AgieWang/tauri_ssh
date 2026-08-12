# Codex CLI 桥接用法

仅在 `SKILL.md` 的显式触发和安全边界均满足时读取本文件。

## 调用入口

在项目根目录执行：

```bash
python .codex/skills/collaborating-with-codex/scripts/codex_bridge.py \
  --cd . \
  --sandbox read-only \
  --PROMPT "Analyze the requested code scope and return evidence with file locations."
```

桥接脚本返回 JSON，常用字段为 `success`、`SESSION_ID`、`agent_messages` 和 `error`。

## 参数

| 参数 | 必填 | 默认值 | 用途与限制 |
|---|---|---|---|
| `--PROMPT` | 是 | — | 任务说明；不得包含凭据或无关敏感内容 |
| `--cd` | 是 | — | 已授权工作区根目录 |
| `--sandbox` | 否 | `read-only` | 必须保持 `read-only` |
| `--SESSION_ID` | 否 | 无 | 继续用户明确要求的已有会话 |
| `--return-all-messages` | 否 | `False` | 仅故障排查时使用，输出前检查敏感内容 |
| `--image` | 否 | 无 | 仅附加任务范围内且不含敏感信息的图片 |
| `--model` | 否 | 无 | 仅用户明确指定模型时传入 |
| `--profile` | 否 | 无 | 仅用户明确指定已知安全配置时传入 |
| `--skip-git-repo-check` | 否 | 脚本默认值 | 不得借此扩大工作目录 |
| `--yolo` | 否 | `False` | 禁止使用 |

## 常见只读模式

### 代码分析

```bash
python .codex/skills/collaborating-with-codex/scripts/codex_bridge.py \
  --cd . \
  --sandbox read-only \
  --PROMPT "Trace the requested execution path. Cite files and distinguish facts from assumptions."
```

### 审查候选差异

先由当前代理获得本次任务的明确 diff 范围，再请求外部审查：

```bash
python .codex/skills/collaborating-with-codex/scripts/codex_bridge.py \
  --cd . \
  --sandbox read-only \
  --PROMPT "Review only the requested change scope for correctness, security, and regressions. Return findings with file locations."
```

### 请求 Unified Diff 候选

```bash
python .codex/skills/collaborating-with-codex/scripts/codex_bridge.py \
  --cd . \
  --sandbox read-only \
  --PROMPT "Generate a minimal Unified Diff candidate only. Do not modify files. OUTPUT: Unified Diff Patch ONLY."
```

该 Diff 不得直接应用；必须先检查路径、上下文、未提交改动、依赖和测试影响。

### 多轮会话

首轮保存返回的 `SESSION_ID`，后续仅在同一授权任务中继续：

```bash
python .codex/skills/collaborating-with-codex/scripts/codex_bridge.py \
  --cd . \
  --sandbox read-only \
  --SESSION_ID "uuid-from-previous-response" \
  --PROMPT "Re-evaluate the previous proposal against the additional evidence."
```

## 输出复核清单

- 文件路径、函数名和行文上下文在当前工作区中真实存在。
- 建议没有覆盖其他会话未提交改动。
- 没有绕过 Command → Service → Database、API 封装、Capabilities 或项目安全边界。
- 数据库、字段、接口和版本结论有当前证据。
- 补丁不包含 `unwrap()`、裸 `invoke()`、明文凭据或越权外部访问。
- 建议涉及的检查和测试已由当前代理真实执行。

## 故障排查

| 现象 | 处理 |
|---|---|
| `codex: command not found` | 报告缺少 CLI；不要自动全局安装 |
| 认证不可用 | 提示用户自行完成安全登录；不要读取或回显 Key |
| `SESSION_ID` 无效 | 重新开始只读会话，或请用户确认要继续的会话 |
| 输出截断 | 在确认不会扩大敏感输出后使用 `--return-all-messages` |
| 路径错误 | 使用当前项目绝对根目录核对 `--cd`，不扫描上级目录 |
| 外部建议与仓库不符 | 以当前仓库事实为准，丢弃建议并记录差异 |

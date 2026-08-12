# Frontmatter 与路由规范

## Frontmatter

每个入口只保留两个字段：

```yaml
---
name: skill-name
description: 说明 Skill 做什么、何时使用以及何时不使用。
---
```

- `name` 使用小写字母、数字和连字符，目录必须同名，建议不超过 64 个字符。
- `description` 是主要触发面，应包含能力、专有强信号、明确意图和排除场景。
- 不增加触发词清单、版本、作者或平台等自定义 Frontmatter 字段；结构化路由数据归 Manifest。
- 不使用“自动处理所有相关问题”一类无限边界描述。

## 信号分级

- 强信号：领域专有且歧义低，例如 `#[tauri::command]`、`generate_handler!`、`capabilities/*.json`。
- 弱信号：需要与意图或技术层组合，例如“设计”“文件”“检查”“状态”“更新”。
- 排除项：明确记录相邻但不属于本 Skill 的请求。
- 高风险信号：凭据、数据库 DDL、删除、远端写入、发布、签名、权限配置；优先保证召回。

宽泛词不得单独放入 `strongSignals`。先用正反向真实 Prompt 验证信号，再写入 Manifest。

## Manifest 条目

项目自维护 Skill 在 `.codex/skill-routing/manifest.json` 声明：

```json
{
  "name": "tauri-commands",
  "kind": "domain",
  "activation": "auto",
  "intents": ["implement", "refactor"],
  "layers": ["ipc", "rust-command"],
  "strongSignals": ["#[tauri::command]", "generate_handler"],
  "weakSignals": ["Command", "invoke", "IPC"],
  "excludeWhen": ["显式 /command 脚手架"],
  "mutexGroup": "ipc-command-detail",
  "riskTags": [],
  "platforms": ["codex", "claude", "agents"],
  "source": ".codex/skills/tauri-commands/SKILL.md",
  "managed": "project"
}
```

- `kind=workflow-command` 的条目默认 `activation=explicit`，表示仅由显式命令或明确完整工作流请求激活。
- 相互排斥的工作流声明同一 `mutexGroup`，一次只保留最高分候选。
- `managed=upstream` 只检查存在和版本，不由本地同步脚本覆盖。
- `source` 必须位于规范源；`platforms` 只列实际兼容平台。

## 组合与边界

跨层最小完整集放入 `bundles.json`，仅在条件满足时展开。普通单域任务通常 1～2 个 Skill，跨层任务通常 2～4 个；高风险任务不设削弱准确性的硬上限。

新增或调整路由时，至少准备：

1. 明确应命中的正向用例。
2. 词面相似但不应命中的负向用例。
3. 与相邻 Skill 的互斥或组合用例。
4. 高风险场景的保守召回用例。

路由失败时应回退到极简模型评估，不能阻断任务，也不能输出敏感路径或堆栈。

# VitePress 增量同步与元数据

仅在现有站点包含有效 `.docs-meta.json` 时读取。元数据异常时先报告，不盲目全量重建。

## `.docs-meta.json`

元数据位于主项目根，核心结构：

```json
{
  "version": "1.0.0",
  "docsProject": "tauri-ssh-docs",
  "docsLocation": "sibling",
  "docsPath": "../tauri-ssh-docs",
  "themeColor": "#0B6EF0",
  "lastSyncCommit": "<verified-commit>",
  "lastSyncTime": "<iso-8601>",
  "coveredSections": ["guide", "api", "backend", "frontend"],
  "updateHistory": [
    {
      "commit": "<verified-commit>",
      "time": "<iso-8601>",
      "type": "incremental",
      "affectedDocs": ["api/commands.md"],
      "sourceChanges": ["src-tauri/src/commands/example.rs"]
    }
  ]
}
```

写入前校验：

- `docsLocation` 只能是 `sibling`、`internal` 或 `custom`。
- `docsPath` 解析后必须与已授权站点目录一致。
- `lastSyncCommit` 必须存在于当前主仓库；不存在时不能直接 diff。
- `updateHistory` 只追加实际完成的同步记录，不预写成功状态。
- 不在元数据中保存凭据、机器用户名或绝对本机路径。

## 源文件到章节映射

| 主项目路径 | 默认站点章节 |
|---|---|
| `src-tauri/src/commands/**/*.rs` | `api/commands.md` |
| `src-tauri/src/services/**/*.rs` | `backend/services.md` |
| `src-tauri/src/database/**/*.rs` | `backend/database.md` |
| `src-tauri/src/models/**/*.rs` | `api/models.md` |
| `src-tauri/src/error.rs` | `backend/error-handling.md` |
| `src-tauri/Cargo.toml` | `guide/dependencies.md` |
| `src-tauri/tauri.conf.json` | `guide/configuration.md` |
| `src-tauri/capabilities/**/*.json` | `backend/capabilities.md` |
| `src/pages/**/*.tsx` | `frontend/pages.md` |
| `src/store/**/*.ts` | `frontend/state.md` |
| `src/lib/api/**/*.ts` | `frontend/api.md` |
| `package.json` | `guide/dependencies.md` |
| `README.md` | `guide/introduction.md` |
| `AGENTS.md`、`CLAUDE.md`、`.codex/**` | 跳过内部代理指令 |

映射是候选，不是写作事实。需要读取实际 diff 和上下文，聚合同章节的源文件，并识别重命名、删除、类型变化和行为变化。

## 增量同步步骤

1. 读取并校验 `.docs-meta.json`。
2. 只读执行从 `lastSyncCommit` 到当前 HEAD 的文件差异；同时检查未提交业务变化是否应纳入本次文档范围。
3. 按映射聚合章节，读取真实变更和对应调用链。
4. 向用户展示“站点章节 → 源文件 → 预计变化”，确认后写入。
5. 仅更新带稳定生成标记的段落和需要重建的索引表。
6. 保留无标记手工段落、用户新增页面和 `docs/public/` 资源。
7. 构建并在真实浏览器验证后，才更新 `lastSyncCommit`、`lastSyncTime` 和 `updateHistory`。
8. 若部分章节失败，元数据不得把失败部分记为成功；报告剩余范围。

## 全量重建

仅在用户明确要求 `full` 或元数据无法增量恢复且用户同意时执行：

- 基于当前代码重建所有已管理章节。
- 保留手工文件、无标记段落和图片。
- 不删除无法映射的页面。
- `updateHistory[].type` 写为 `full`，并记录实际源范围。
- 构建或浏览器验收失败时，不前移同步检查点。

## 内容保护标记

自动维护区应使用稳定、可配对的 HTML 注释，例如：

```markdown
<!-- docs-management:start section=api-commands -->
自动生成内容
<!-- docs-management:end section=api-commands -->
```

- 不把已有手工段落强行包进生成区。
- 标记缺失、重复或未闭合时停止更新该文件并报告。
- 删除源能力前核对注册、调用和 Git 历史，不能因为一次扫描未命中就删除站点说明。


# Skill 验证

## 结构与编码

- `SKILL.md` 存在，Frontmatter 只含 `name`、`description`。
- name 为 kebab-case，并与目录、Manifest 一致。
- description 写清能力、强触发和排除边界。
- 文件是 UTF-8 无 BOM，无乱码。
- 所有 Markdown 链接存在；入口直接链接必要 references。
- 入口通常 80～180 行，复杂入口不超过 220 行。
- 长 reference 超过 100 行时提供目录。

## 内容完整性

- 修改前的关键禁令、异常路径、测试、构建和运行时验收均可在入口或 references 中找到。
- 没有把数据库真实格式改成样例推断，没有用构建通过替代真实页面或运行时验收。
- Tauri 后端仍遵守 Command → Service → Database；前端 IPC 仍通过 `src/lib/api/`。
- 脚本、示例和引用使用项目当前路径与版本，新增脚本已实际运行。

## 路由验证

至少覆盖以下用例：

- 正向：典型专有表达必须命中。
- 负向：只包含宽泛词或相邻领域表达时不得误命中。
- 互斥：同一工作流阶段最多命中一个阶段 Skill。
- 组合：跨层实现包含最小完整 Skill 集。
- 高风险：凭据、DDL、删除、发布、远端写入和权限变更不得漏选安全门禁。
- 显式命令：普通自然语言不得激活 Manifest 中 `activation=explicit` 的工作流。

## 建议命令

```bash
node .codex/scripts/validate-skills.cjs
node .codex/tests/skill-routing/router.test.cjs
node .codex/scripts/measure-skill-context.cjs
node .codex/scripts/sync-skills.cjs --check
git diff --check
```

若 system `skill-creator` 提供 `quick_validate.py`，还要对改动 Skill 逐个运行。缺少任何脚本时用只读检查补位并如实记录，不将“未运行”写成“通过”。

## 人工复核

1. 检查入口只包含执行所需的最小完整信息。
2. 检查 references 可按任务局部读取，没有深层引用链。
3. 比较相邻 Skill description，确认边界不重叠。
4. 检查 Manifest source、platforms、managed、activation 和互斥组。
5. 检查 `git diff --check` 与变更文件清单，确保没有越界修改。

## 失败处理

- 路由或 Manifest 异常：Hook 回退到极简模型评估，不阻断任务。
- 镜像漂移：修正规范源或 Manifest，再生成镜像；不要手工覆盖。
- 引用缺失：修复链接或恢复内容后重跑。
- 命中回归：先补失败用例，再调整强弱信号和排除项。
- 规则疑似丢失：从版本历史或迁移前完整参考恢复，不能为压缩继续删除。

# 项目专属技能目录（project-skills）

> 本目录与 `.codex/skills/` 互补：
>
> - `.codex/skills/` = 框架本体技能（只读，跟随 framework-sync 更新）
> - `.codex/project-skills/` = 项目专属技能（可写，不参与框架同步）

## 何时往这里加技能？

- 这个技能只对本项目有用，例如与 SSH 连接模型、主机分组、终端会话强耦合的生成器。
- 这个技能包含敏感信息或商业逻辑，不应反哺给框架。

## 何时往 `.codex/skills/` 加技能？

- 不允许直接改框架本体 skills 目录。
- 想给框架加技能时，先在 `project-skills/` 试用，验证通用后再反哺到框架。

## 技能格式

与 `.codex/skills/` 完全一致：每个技能一个目录，含 `SKILL.md`。

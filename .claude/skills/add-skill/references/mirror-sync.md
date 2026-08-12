# 规范源与镜像同步

## 单一规范源

- 项目自维护 Skill：`.codex/skills/<name>/`。
- 路由与平台元数据：`.codex/skill-routing/manifest.json`。
- Claude 镜像：`.claude/skills/<name>/`，由同步脚本生成或校验。
- Agents 镜像：`.agents/skills/<name>/`，仅同步 Manifest 声明兼容的 Skill。
- Claude command：由 `kind=workflow-command` 的规范源正文生成或校验；Codex 仍使用带 Frontmatter 的 Skill 入口。

不要再维护 Hook 硬编码 Skill 清单，也不要用 `cp` 手工复制三个目录。

## 同步流程

1. 修改 `.codex` 规范源和 Manifest。
2. 先运行只读检查：

```bash
node .codex/scripts/sync-skills.cjs --check
```

3. 审查将创建、修改或删除的精确目标。
4. 仅在用户授权后写入：

```bash
node .codex/scripts/sync-skills.cjs --write
```

5. 再次运行 `--check`，并执行路由与格式验证。

默认行为必须是只读检查；`--write` 才能改镜像。脚本不得改规范源、业务代码、用户全局 Skill 或未列入 `platforms` 的目录。

同一仓库不支持并发执行多个 `--write`/`--prune` 同步进程。一次只运行一个写入者；执行前后均重新 `--check`，避免本地目录在安全检查与原子替换之间被其他进程切换。

## Upstream 例外

官方或外部生成的 Skill 在 Manifest 标记 `managed=upstream`。同步脚本只检查其存在、入口名称和版本兼容性，不覆盖正文。更新时使用对应官方同步机制，再重新执行项目验证。

## 漂移处理

发现同名镜像不一致时：

1. 不直接选择较新时间戳或较大文件作为真相。
2. 比较三份规则，识别仍有效的安全、数据库、测试和平台差异。
3. 将保留内容合并回 `.codex` 规范源及 references。
4. 更新 Manifest 后由脚本生成镜像。
5. 运行 `--check` 证明漂移为零。

对于大小写为 `skill.md` 的历史入口，先验证各运行时兼容性，再由同步流程统一为 `SKILL.md`。

## 删除与重命名

- 同步器只管理当前 Manifest 记录的资源树，不从“记录被删除或改名”推断旧文件的所有权。
- 删除条目或重命名后，旧镜像与旧 Claude command 变成 unlisted，`--check`/`--write`/`--write --prune` 都必须保留它们。
- 整项清理必须是独立的显式删除步骤：在旧配置仍可读取时列出每个精确源、镜像资源和 command，获得用户对这些目标的授权，再逐文件复核根目录、符号链接和文件类型后删除。
- 新 Manifest 不能通过伪装成 project/旧名称来取得 platform-local、upstream 或原 unlisted 文件的删除权。
- `--prune` 只处理当前 project-managed Skill 精确目录内、相对于当前规范源资源树多出的文件；不承担整个旧 Skill 的删除。

## 安全边界

- 不在命令行、日志、Skill 或 Manifest 中写入 Token、密码、私钥或真实凭据。
- 不借同步执行 Git push、发布、服务器或数据库写入。
- 不使用 stash、reset、跨分支 checkout、全量 add 或清理命令处理镜像差异。
- 其他会话的未提交文件不属于同步目标。

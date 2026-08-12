# VitePress 站点工作流

仅在显式 `/update-docs` 对应子命令执行时读取。站点模板、占位符和源码映射以 `docs-management` 当前规范为准。

## 状态机

```text
读取主项目根 .docs-meta.json
├─ status / diff：只读
├─ 不存在 + init/无参数：收集信息，展示计划，确认后初始化
├─ 存在 + update/无参数：按 lastSyncCommit 增量影响分析
└─ 存在 + full：全部受管章节重建预览
```

## 初始化

至少确认：

- 站点路径（sibling、`website/` 或明确自定义路径）。
- 对外项目名、一句话描述、主题色；站点 URL/Logo 可选。
- 目标路径是否已有文件/仓库，以及哪些内容必须保留。

确认后：

1. 从项目现有模板复制站点骨架；模板不存在则停止，不临时发明另一套结构。
2. 生成主项目根 `.docs-meta.json`，记录 docsPath、mode、lastSyncCommit、lastSyncTime、coveredSections。
3. 从真实 `tauri.conf.json`、Cargo/package 配置、README 和 Command 注册生成首版受管章节。
4. 构建并在浏览器检查首页、导航和关键页面。

是否初始化 Git/commit 必须由用户明确指定；绝不自动 push。

## 增量更新

1. 校验 `lastSyncCommit` 存在且是当前仓库可达提交。
2. 使用 `git diff --name-only <lastSyncCommit>..HEAD` 获取已提交变化，并单独列出未提交相关变化。
3. 按 `docs-management` 映射源文件到聚合章节，展示影响表。
4. 只重写受管标记段，更新索引和元数据；用户段落保持原样。
5. 成功验证后才把同步基线推进到实际覆盖的 commit。

## 全量重建

- 重新生成 `coveredSections` 内受管内容，不删除其他章节。
- 保留 `public/`、用户新增 Markdown 和非受管段。
- 写入前给出文件清单和备份/补丁回滚方式。
- `updateHistory.type` 记录为 `full`，同步基线规则与增量一致。

## 状态与预览

`status` 输出 docsPath、mode、lastSyncCommit/time、受管章节和距基线变更数；`diff` 额外输出源文件 → 文档章节映射。两者不得修改元数据或站点。

## 验证门

- docsPath 和元数据互相指向正确。
- 站点依赖安装状态明确，构建命令成功。
- 内部链接、导航、代码示例和版本信息来自当前源码。
- 浏览器无阻断错误，首页与至少一个受影响章节通过验收。
- 当前修改可通过 patch 恢复，且没有目标外文件进入 diff。

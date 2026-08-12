# 项目管理文档同步参考

仅在显式 `/sync` 时按需读取。

## 最小扫描口径

```bash
git status -s
git log -20 --format='%H|%an|%cn|%s' --no-merges
rg --files src-tauri/src/commands src-tauri/src/services src-tauri/src/database src-tauri/src/models
rg --files src/pages src/components src/store src/lib/api
rg -n 'TODO|FIXME|todo!\(|unimplemented!\(' src-tauri/src src
rg --files docs/tasks/active docs/architecture
```

目录不存在时记录为“不适用”，不能把命令失败解释为零项。

## 差异分类

| 类型                          | 处理                                       |
| ----------------------------- | ------------------------------------------ |
| 代码/任务有明确新增，文档缺失 | 提议新增，展示来源文件                     |
| 文档记录的文件或符号已不存在  | 标为疑似过期，检查重命名/迁移后再决定      |
| TODO 已从代码删除             | 不能直接判定完成，结合任务、测试和提交证据 |
| 文档与真实运行结果冲突        | 以运行证据形成待确认项，不静默覆盖         |
| 人工说明无法机器验证          | 原样保留                                   |

## 更新报告模板

```markdown
# 项目文档同步报告

## 结论

- 已确认更新：...
- 待人工确认：...
- 未修改：...

## 证据范围

| 来源 | 版本/时间 | 结果 | 局限 |
| ---- | --------- | ---- | ---- |

## 文件变更

| 文档 | 修改内容 | 为什么修改 | 保留内容 | 回滚方式 |
| ---- | -------- | ---------- | -------- | -------- |

## 冲突与未知

| 文档陈述 | 代码/任务证据 | 处理 |
| -------- | ------------- | ---- |
```

## 验证

- 文档引用的实际路径存在，或明确标记为计划路径。
- 数字重新统计，不复用旧报告值。
- Markdown 表格和链接可读，UTF-8 无 BOM，无乱码。
- `git diff --check` 通过，diff 中没有目标外文件。

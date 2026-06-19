---
name: update-status
description: |
  /update-status - 智能更新项目状态。自动分析 Git 提交和代码变更，联动更新项目状态、待办清单、需求文档。

  触发场景：
  - 需要更新项目管理文档
  - 需要同步代码变更到项目状态
  - 需要批量更新待办清单状态
  - 需要联动更新三个核心文档

  触发词：更新状态、更新项目、update-status、项目状态更新
user_invocable: true
---

# /update-status - 智能更新项目状态

作为项目状态更新助手，智能更新项目管理文档。支持自动创建缺失文档、联动更新三个文档。

---

## 第一步：智能检测文档状态

### 1.1 检查三个核心文档

检查 `docs/` 目录下是否存在以下文件：
- `docs/项目状态.md`
- `docs/待办清单.md`
- `docs/需求文档.md`

### 1.2 自动创建缺失文档

**如果 `docs/项目状态.md` 不存在**：
1. 读取模板 `.claude/templates/项目状态模板.md`（如有）
2. 扫描 Rust 后端模块和 React 前端页面获取初始数据
3. 创建 `docs/项目状态.md`
4. 输出提示：`已自动创建 docs/项目状态.md`

**如果 `docs/待办清单.md` 不存在**：
1. 读取模板 `.claude/templates/待办清单模板.md`（如有）
2. 扫描代码中的 TODO/FIXME/todo!/unimplemented! 作为初始待办
3. 创建 `docs/待办清单.md`
4. 输出提示：`已自动创建 docs/待办清单.md`

**如果 `docs/需求文档.md` 不存在**：
1. 读取模板 `.claude/templates/需求文档模板.md`（如有）
2. 从 `Cargo.toml` 和 `package.json` 提取技术栈信息
3. 创建 `docs/需求文档.md`
4. 输出提示：`已自动创建 docs/需求文档.md`

---

## 第二步：分析最新状态

### 2.1 读取现有文档
- 读取 `docs/项目状态.md` 的当前内容
- 记录"已完成"、"进行中"、"待办"各区域的任务

### 2.2 分析 Git 提交（过滤非开发提交）

```bash
# 获取最近 20 条提交，包含 author/committer/message 信息
git log -20 --format="%H|%an|%cn|%s" --no-merges
```

**过滤规则**（以下提交不计入开发活动统计）：

| 过滤条件 | 识别方式 | 分类 |
|---------|---------|------|
| Cherry-pick 提交 | commit message 包含 `cherry picked from` | 同步提交 |
| 上游同步提交 | commit message 以 `sync:` 或 `upstream:` 开头 | 同步提交 |
| Author ≠ Committer | author 和 committer 名字不同 | 可能是 cherry-pick |
| 文档/配置提交 | commit message 以 `docs:` `chore:` `style:` 开头 | 非功能提交 |

- 识别过滤后的业务相关提交（feat/fix/update）
- 提取功能描述和完成日期

### 2.3 扫描项目模块

**按 Rust 后端模块统计**：
- 扫描 `src-tauri/src/commands/` — Command 模块（IPC 入口）
- 扫描 `src-tauri/src/services/` — Service 层（业务逻辑）
- 扫描 `src-tauri/src/database/` — Database 层（数据访问）
- 扫描 `src-tauri/src/models/` — 数据模型定义
- 检查每个模块的完整性（command + service + model）
- 识别新完成的功能

**按 React 前端页面统计**：
- 扫描 `src/pages/` 或 `src/views/` — 页面组件
- 扫描 `src/components/` — 通用组件
- 扫描 `src/store/` — Zustand 状态管理
- 扫描 `src/lib/api/` — API 封装层

**扫描 TODO/FIXME/todo!/unimplemented!**：
- 扫描 Rust 代码中的 `todo!()`, `unimplemented!()`, `// TODO`, `// FIXME`
- 扫描 TypeScript/React 代码中的 `// TODO`, `// FIXME`, `// XXX`
- 记录文件路径和行号
- 与现有待办对比，识别新增/已解决

### 2.4 读取活跃任务文档

```bash
# 扫描活跃任务
ls docs/tasks/active/
```

对每个活跃任务文档，读取以下信息：

| 读取区域 | 提取信息 |
|---------|---------|
| 文件头部 | 任务名称、状态（进行中/暂停）、最后更新时间 |
| 实现步骤 | `- [ ]` 和 `- [x]` 统计，计算步骤完成率 |
| 当前进度 | "正在进行"、"下一步操作" |
| 问题记录 | 未解决问题数量 |

**注意**：如果 `docs/tasks/active/` 不存在或为空，跳过此数据源。

---

## 第三步：更新项目状态.md

### 3.1 更新"已完成"区域
- 将新完成的任务添加到"已完成"
- 从 Git 提交获取完成日期
- 计算耗时（如果有开始日期）

### 3.2 更新"进行中"区域
- 更新进行中任务的进度百分比
- 检测长期未更新的任务（超过7天提醒）

### 3.3 更新"待办"区域
- 添加新发现的 TODO/todo!/unimplemented! 任务
- 标注优先级和所属模块（Rust 后端 / React 前端）

### 3.4 更新"活跃任务"区域

如果 `docs/tasks/active/` 存在且有任务文档：
1. 在 `项目状态.md` 中查找"活跃任务"区域（如不存在则在"进行中"区域后新增）
2. 汇总每个活跃任务的信息：

```markdown
## 活跃任务（来自 task-tracker）

| 任务 | 状态 | 进度 | 最后更新 |
|------|------|------|---------|
| {任务名} | 进行中 | X/Y (Z%) | YYYY-MM-DD |
```

3. 如果有已归档的任务（docs/tasks/archive/），统计本月归档数量

### 3.5 更新统计信息
- 更新"最后更新"时间
- 更新"整体进度"百分比
- 更新"下一步计划"

---

## 第四步：联动更新待办清单.md

### 4.1 同步已完成任务

当项目状态.md 中有任务标记为"已完成"时：
1. 在待办清单.md 中查找对应任务
2. 将任务移动到"最近完成"区域
3. 添加完成日期和耗时

**匹配规则**：
- 按任务名称模糊匹配
- 按模块名匹配（如 commands/、services/、pages/）
- 按关键词匹配（如"窗口管理"、"IPC 通信"等）

### 4.2 同步新增待办

当扫描到新的 TODO/FIXME/todo!/unimplemented! 时：
1. 按优先级分类添加到待办清单
   - FIXME/unimplemented!/URGENT/CRITICAL → 高优先级
   - TODO/todo! → 中优先级
   - OPTIMIZE/ENHANCE → 低优先级
2. 记录来源文件和行号
3. 标注所属层（Rust Core / React UI）

### 4.3 更新统计信息
- 更新"待办总数"
- 更新"本周完成"
- 计算"完成率"

---

## 第五步：联动更新需求文档.md

### 5.1 同步需求状态

当项目状态.md 中有功能标记为"已完成"时：
1. 在需求文档.md 中查找对应需求（REQ-xxx）
2. 将需求状态从"待开发/开发中"改为"已完成"
3. 记录实际完成日期

### 5.2 更新技术需求

如果检测到 `Cargo.toml` 或 `package.json` 依赖版本变更：
- 更新需求文档中的技术栈版本（Tauri、React、Rust crates 等）

---

## 第六步：输出更新报告

```markdown
# 项目状态更新报告

**更新时间**: YYYY-MM-DD HH:MM（东八区 UTC+8）

## 文档状态
- docs/项目状态.md - 已更新
- docs/待办清单.md - 已同步
- docs/需求文档.md - 已同步

## 本次变更

### 项目状态.md
- 新增已完成: X 项
- 更新进行中: X 项
- 新增待办: X 项

### 待办清单.md（联动更新）
- 移至已完成: X 项
- 新增待办: X 项
- 当前待办总数: X 项

### 需求文档.md（联动更新）
- 状态更新: X 个需求

## 主要完成内容
1. [具体完成的功能]
2. [具体完成的功能]

## 发现的问题
- [长期未更新的任务]（超过7天）
- [建议处理的高优先级待办]

## 活跃任务汇总（来自 task-tracker）
{如果有活跃任务}
| 任务 | 状态 | 进度 | 最后更新 |
|------|------|------|---------|
| {任务名} | 进行中 | X/Y (Z%) | YYYY-MM-DD |

{如果没有活跃任务则省略此区域}

## 下一步

> 运行 `/next` 获取具体的下一步开发建议
```

---

## 注意事项

### 保留用户手动内容
- 更新时保留用户手动添加的内容
- 只更新可以从代码/Git自动分析的信息
- 存疑时不覆盖，而是追加

### 智能匹配规则
- 任务匹配采用模糊匹配，容忍名称小差异
- 优先使用模块名+功能名组合匹配
- 匹配不确定时，在报告中提示人工确认

### Tauri 项目特殊扫描点
- Rust 后端：`src-tauri/src/commands/`、`src-tauri/src/services/`、`src-tauri/src/database/`、`src-tauri/src/models/`
- React 前端：`src/pages/`、`src/components/`、`src/store/`、`src/lib/api/`
- 配置文件：`src-tauri/tauri.conf.json`、`src-tauri/capabilities/`
- Rust 特有标记：`todo!()`、`unimplemented!()`、`unreachable!()`

### 时间和格式
- 时间格式: `YYYY-MM-DD HH:MM`
- **时区: 必须使用东八区（UTC+8，Asia/Shanghai）时间**，获取当前时间时使用 `TZ=Asia/Shanghai date '+%Y-%m-%d %H:%M'`
- 进度百分比: 0-100%，整数
- 工作量: X天，整数估算

---

## 与其他命令的关系

| 命令 | 关系 |
|------|------|
| `/start` | 初始化时创建文档，后续由 /update-status 维护 |
| `/progress` | 只读分析，/update-status 会修改文档 |
| `/next` | /update-status 更新文档后，推荐运行 /next 获取下一步建议 |
| `/sync` | 全量同步，/update-status 是增量更新 |
| `task-tracker` | /update-status 读取活跃任务汇总到项目状态文档 |

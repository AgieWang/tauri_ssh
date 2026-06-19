---
name: sync
description: |
  /sync - 同步项目文档

  触发场景：
  - 需要全量同步项目管理文档
  - 需要检查文档之间的一致性
  - 需要基于代码扫描更新所有文档
  - 项目文档出现不一致需要修复

  触发词：/sync、同步文档、全量同步、文档同步、数据一致性、同步项目、文档更新
---

# /sync - 同步项目文档

全量同步项目管理的三个核心文档，确保数据一致性。

## 功能概述

- 全量扫描 Rust 后端和 React 前端代码及 Git 记录
- 同步三个核心文档的数据
- 检测并修复文档间的不一致
- 生成完整的同步报告

---

## 核心文档

| 文档 | 路径 | 用途 |
|------|------|------|
| 项目状态 | `docs/项目状态.md` | 模块进度、里程碑、活动记录 |
| 待办清单 | `docs/待办清单.md` | TODO/FIXME/todo!、任务优先级 |
| 需求文档 | `docs/需求文档.md` | 功能需求、验收标准 |

---

## 执行流程

### 第一步：检查文档状态

```bash
# 检查三个核心文档
FILES=(
    "docs/项目状态.md"
    "docs/待办清单.md"
    "docs/需求文档.md"
)

for file in "${FILES[@]}"; do
    if [ ! -f "$file" ]; then
        echo "缺少文档: $file"
        MISSING_DOCS+=("$file")
    fi
done

# 如果有缺失，建议创建
if [ ${#MISSING_DOCS[@]} -gt 0 ]; then
    echo "建议先运行 /start 或手动创建缺失的文档"
fi
```

### 第二步：扫描代码结构

```bash
# === Rust 后端扫描 ===

# 扫描 Tauri Commands（IPC 入口）
COMMAND_FILES=$(find src-tauri/src/commands -name "*.rs" ! -name "mod.rs" 2>/dev/null)
COMMAND_COUNT=$(echo "$COMMAND_FILES" | grep -c ".rs$" 2>/dev/null || echo 0)

# 统计每个 Command 文件中的 #[tauri::command] 数量
for file in $COMMAND_FILES; do
    CMD_FN_COUNT=$(grep -c "#\[tauri::command\]" "$file" 2>/dev/null || echo 0)
    echo "$(basename $file .rs): $CMD_FN_COUNT commands"
done

# 扫描 Services 层
SERVICE_FILES=$(find src-tauri/src/services -name "*.rs" ! -name "mod.rs" 2>/dev/null)
SERVICE_COUNT=$(echo "$SERVICE_FILES" | grep -c ".rs$" 2>/dev/null || echo 0)

# 扫描 Database 层
DB_FILES=$(find src-tauri/src/database -name "*.rs" ! -name "mod.rs" 2>/dev/null)
DB_COUNT=$(echo "$DB_FILES" | grep -c ".rs$" 2>/dev/null || echo 0)

# 扫描 Models
MODEL_FILES=$(find src-tauri/src/models -name "*.rs" ! -name "mod.rs" 2>/dev/null)
MODEL_COUNT=$(echo "$MODEL_FILES" | grep -c ".rs$" 2>/dev/null || echo 0)

# === React 前端扫描 ===

# 扫描页面组件
PAGE_COUNT=$(find src/pages -name "*.tsx" -o -name "*.ts" 2>/dev/null | wc -l)

# 扫描通用组件
COMPONENT_COUNT=$(find src/components -name "*.tsx" 2>/dev/null | wc -l)

# 扫描 Zustand Store
STORE_COUNT=$(find src/store -name "*.ts" 2>/dev/null | wc -l)

# 扫描 API 封装
API_COUNT=$(find src/lib/api -name "*.ts" 2>/dev/null | wc -l)
```

### 第三步：扫描 TODO/FIXME

```bash
# 扫描 Rust 后端
# todo!() 宏 — Rust 特有的未实现标记
RUST_TODO_MACRO=$(grep -rn "todo!()" src-tauri/src/ --include="*.rs" 2>/dev/null)
RUST_UNIMPL=$(grep -rn "unimplemented!()" src-tauri/src/ --include="*.rs" 2>/dev/null)
# 注释形式的 TODO/FIXME
RUST_TODO=$(grep -rn "// TODO\|// FIXME" src-tauri/src/ --include="*.rs" 2>/dev/null)

# 扫描 React 前端
TS_TODO=$(grep -rn "// TODO\|// FIXME\|// XXX" src/ --include="*.ts" --include="*.tsx" 2>/dev/null)
```

### 第四步：分析 Git 记录

```bash
# 获取最近 30 天的提交统计
git log --since="30 days ago" --pretty=format:"%s" | head -30

# 按目录统计（区分 Rust 后端和 React 前端）
echo "=== Rust 后端活动 ==="
git log --since="30 days ago" --name-only --pretty=format: | \
    grep -E "^src-tauri/" | \
    cut -d'/' -f3 | sort | uniq -c | sort -rn

echo "=== React 前端活动 ==="
git log --since="30 days ago" --name-only --pretty=format: | \
    grep -E "^src/" | \
    cut -d'/' -f2 | sort | uniq -c | sort -rn

# 获取最近提交
RECENT_COMMITS=$(git log --oneline -10)
```

### 第五步：检测文档冲突

```bash
# 比较项目状态中的模块列表与实际代码
# 检测已删除但文档中仍存在的模块
# 检测新增但文档中未记录的模块

# 比较待办清单中的 TODO 与代码中的 TODO
# 检测已完成但未标记的 TODO
# 检测新增的 TODO

# 检查 Cargo.toml 和 package.json 依赖变更
```

### 第六步：更新文档

```bash
CURRENT_TIME=$(TZ=Asia/Shanghai date '+%Y-%m-%d %H:%M')

# 1. 更新项目状态
# - 更新时间戳
# - 更新 Rust 后端模块统计（Commands/Services/Database/Models）
# - 更新 React 前端统计（Pages/Components/Store/API）
# - 添加最近活动

# 2. 更新待办清单
# - 同步新的 TODO/FIXME/todo!/unimplemented!
# - 标记已完成的项目

# 3. 更新需求文档（如果有新模块）
# - 添加新 Command 的功能描述
# - 更新 Cargo.toml / package.json 依赖版本
```

---

## 输出格式

```markdown
## 项目文档同步报告

**同步时间**: 2026-03-20 15:30:00
**扫描范围**: 全量

---

### 代码统计

#### Rust 后端（src-tauri/src/）

| 模块 | 文件数 | Command 数 | Service 数 | 完整度 |
|------|--------|-----------|-----------|--------|
| commands/ | X | X | - | - |
| services/ | X | - | X | - |
| database/ | X | - | - | - |
| models/ | X | - | - | - |
| **合计** | **X** | **X** | **X** | **X%** |

#### React 前端（src/）

| 模块 | 文件数 | 状态 |
|------|--------|------|
| pages/ | X | 正常 |
| components/ | X | 正常 |
| store/ | X | 正常 |
| lib/api/ | X | 正常 |

---

### TODO/FIXME 统计

| 类型 | Rust 后端 | React 前端 | 合计 |
|------|----------|-----------|------|
| TODO / todo!() | X | X | X |
| FIXME | X | X | X |
| unimplemented!() | X | - | X |

**新发现**（本次同步新增）：
- `[todo!()]` 待实现文件导出 - `src-tauri/src/services/export.rs:45`
- `[FIXME]` 修复窗口焦点问题 - `src/pages/settings.tsx:128`

---

### Git 活动（最近 30 天）

**提交统计**：X 次提交

| 模块 | 提交数 | 占比 |
|------|--------|------|
| src-tauri (Rust) | X | X% |
| src (React) | X | X% |

**最近提交**：
- `abc1234` feat: 添加窗口管理 Command
- `def5678` fix: 修复 IPC 序列化问题
- `ghi9012` perf: 优化 SQLite 查询性能

---

### 文档冲突检测

| 问题 | 详情 | 处理方式 |
|------|------|---------|
| 模块不存在 | 项目状态中记录了某 Command，但代码中不存在 | 已删除记录 |
| 新模块未记录 | 发现新 Command/Service，未在项目状态中 | 已添加 |
| TODO 已完成 | 待办清单中的 X 个 TODO 在代码中已删除 | 已标记完成 |

---

### 同步结果

| 文档 | 更新项 | 状态 |
|------|--------|------|
| 项目状态.md | 时间戳、模块统计、活动记录 | 已更新 |
| 待办清单.md | 新增 X 条、完成 X 条 | 已更新 |
| 需求文档.md | 添加模块描述 | 已更新 |

---

### 建议

1. **紧急**：处理 X 个 FIXME / unimplemented!()
2. **重要**：清理 X 个 TODO / todo!()
3. **建议**：完善 Command 的需求描述
```

---

## 注意事项

### 1. 扫描范围

**扫描业务代码**：
```
src-tauri/src/commands/     -- Tauri Command 定义
src-tauri/src/services/     -- 业务逻辑层
src-tauri/src/database/     -- 数据库访问层（rusqlite）
src-tauri/src/models/       -- 数据模型
src/pages/                  -- React 页面组件
src/components/             -- 通用 UI 组件
src/store/                  -- Zustand 状态管理
src/lib/api/                -- API 封装（invoke 调用）
```

**不扫描框架核心**：
```
src-tauri/src/lib.rs        -- Tauri 应用入口（框架胶水代码）
src-tauri/src/main.rs       -- 主函数
src-tauri/capabilities/     -- 安全权限声明（配置文件）
node_modules/               -- 依赖
target/                     -- Rust 编译产物
```

### 2. 冲突处理

| 冲突类型 | 处理方式 |
|---------|---------|
| 文档中有、代码中无 | 询问用户是否删除 |
| 代码中有、文档中无 | 自动添加 |
| TODO/todo! 已删除 | 自动标记完成 |

### 3. 备份机制

同步前会自动备份原文档到 `docs/.backup/` 目录。

---

## 与其他命令的配合

| 场景 | 推荐流程 |
|------|---------|
| 每周同步 | `/sync` -> 查看报告 -> 处理问题 |
| 项目初始化 | `/start` -> `/sync` |
| 功能开发后 | `/dev` 完成 -> `/sync` 同步 |
| 版本发布前 | `/sync` -> `/check` -> `/progress` |

---
name: project-navigator
description: |
  用于理解仓库结构、寻找功能入口或追踪 React 到 Rust/数据库的真实调用链。

  触发场景：
  - 用户需要了解项目或模块的目录结构与职责
  - 不知道功能代码位置，需要寻找入口和相关文件
  - 需要追踪页面、API、Command、Service、Database 的调用链
  - 接手陌生模块，需要建立只读事实地图

  触发词：仓库结构、功能入口、代码位置、调用链追踪、模块导航、从页面追后端、定位实现文件
---

# Tauri 项目导航

## 能力边界

本 Skill 负责只读定位和结构理解，不负责提出架构重构或直接实现功能。

- 用户已给出精确文件路径且只要求局部修改时，通常不额外加载。
- 模块边界或依赖方向设计使用 `architecture-design`。
- 故障根因定位使用 `bug-detective`；本 Skill 可为其提供调用链事实。
- 应用文件系统功能使用 `file-storage`，普通仓库文件查找不使用它。

## 导航原则

1. 先用 `rg --files` 和 `rg` 搜索符号、路由、命令名、表名和配置键。
2. 从真实入口向下追踪，不只根据目录名称猜测职责。
3. 同时查定义、注册、调用、类型、持久化和测试，避免只找到其中一层。
4. 区分当前事实与可能影响；导航阶段不做未经证据支持的设计结论。
5. 保留其他会话未提交改动，不写文件、不切分支、不执行清理动作。
6. 目录图只是导航假设；每次使用都动态读取当前树，不能固化旧的 `index.ts`、`mod.rs` 或单文件结构。

## 标准追踪流程

### 1. 确定用户入口

- 页面：从 `src/Router.tsx`、侧边栏或页面组件定位路由和交互。
- API：从 `src/lib/api/` 查封装方法与 `invoke` 名称。
- 后端：从 `#[tauri::command]` 和 `generate_handler!` 查定义与注册。
- 数据：从 Service 追到 Database、schema、迁移和 Model。
- 共享基础设施：检查 `src-tauri/src/shared/` 及其导出，不把跨域基础能力误归入单个 Service。
- 系统能力：追到插件注册、Capabilities、配置和资源文件。

### 2. 双向搜索

- 向下：入口 -> 调用 -> 实现 -> 数据/外部能力。
- 向上：被修改符号 -> 所有调用者 -> 用户可见行为。
- 横向：类型定义、测试、配置、日志和相似实现。

### 3. 核对契约

- Rust/TypeScript DTO 字段、命名、可空性和枚举值；当前 `src/types/`、`src/store/`、`src/lib/api/` 已按领域拆分，应追踪领域文件和聚合出口两处。
- Command 名称、参数转换、返回类型和错误约定；当前结构化错误链为 Rust `CommandError { code, message }` 与前端 `parseCommandError/getErrorCode/getErrorMessage`。
- SQLite 表、迁移版本、查询条件和真实数据格式。
- Capabilities、插件权限与窗口作用域。

### 4. 记录影响面

按“直接入口、核心实现、数据/配置、调用者、测试/验收”分组列出文件。对每个文件说明为什么相关，避免只输出一串路径。

## 常用搜索模式

```bash
# 文件清单与模块入口
rg --files src src-tauri

# Tauri Command 定义、注册和调用
rg -n '#\[tauri::command\]|generate_handler!' src-tauri/src
rg -n 'invoke\(|command_name' src

# 数据库表、迁移和模型
rg -n 'CREATE TABLE|ALTER TABLE|user_version|table_name' src-tauri/src

# 路由、导航和页面
rg -n 'path:|Route|navigate\(|menu' src
```

搜索范围和符号应替换为当前任务的真实名称；`rg` 不可用时再选择替代工具。

## 输出格式

事实地图至少包含：

1. 用户可见入口与触发动作。
2. 前端组件、状态与 API 封装。
3. Command 注册、Service 和 Database 调用链。
4. Model/DTO、schema、配置和权限。
5. 现有测试、运行方式与相似实现。
6. 直接影响文件、间接影响文件和仍未知项。

## 不应触发示例

- “修改 `/exact/path/file.ts` 第 20 行的文案。”
- “实现文件选择和导入功能。”
- “重新设计知识模块的分层职责。”
- “这个页面为什么加载失败？”

## 按需参考

需要完整项目目录图、文件职责和功能定位速查时，读取 [references/repository-map.md](references/repository-map.md)。该图只记录当前模块化模式，目录可能继续演进；每次使用前必须以 `rg --files` 和当前导出关系验证。

## 完成条件

- 已找到入口、注册、实现、数据和调用者，而非孤立文件。
- 每个相关文件都有基于源码的关联理由。
- 当前事实、推测和未知项被清楚区分。
- 输出足以支持后续诊断、设计或实现，但导航阶段没有擅自修改文件。

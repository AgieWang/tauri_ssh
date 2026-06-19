---
name: add-skill
description: |
  当需要为框架增加新技能、修改现有技能、编写技能文档时自动使用此 Skill。

  触发场景：
  - 需要为新模块添加技能
  - 需要为新功能编写技能文档
  - 需要扩展框架的技能系统
  - 需要将实现步骤转化为可复用技能
  - 需要修改现有技能内容并同步到双系统
  - 需要重命名或删除现有技能

  触发词：添加技能、创建技能、新技能、技能开发、写技能、技能文档、skill 创建、修改技能、更新技能、同步技能、技能同步
---

# 技能创建与维护指南

## 概述

本指南用于在 Tauri 桌面应用框架中**添加新技能**和**维护现有技能**（修改、重命名、删除）。技能是框架的核心能力，通过自动评估和激活，确保 Rust 后端与 React 前端的代码风格和规范一致性。

本框架采用双进程架构（WebView 进程 + Rust Core 进程），技能覆盖以下领域：
- **Rust 后端**：Command / Service / Database / Plugin 开发
- **React 前端**：Ant Design 组件 / Zustand 状态 / React Router 路由
- **Tauri 特性**：Capabilities / Events / Window 管理 / 打包分发 / 自动更新
- **跨领域**：IPC 通信 / 错误处理 / 性能优化 / 测试

> **核心原则**：`.claude/skills/` 是主目录（source of truth），`.codex/skills/` 是镜像。任何技能的新增或修改，都必须同步到两个目录。

---

## Claude Commands 与 Codex Skills 的对应关系

> Codex **没有** commands 系统。Claude 的每个 command（`.claude/commands/xxx.md`）必须在 Codex 中以 skill（`.codex/skills/xxx/SKILL.md`）的形式存在。

**核心区别**：
- **Skill**：两端文件完全相同（直接复制）
- **Command**：Claude 是纯 Markdown（无 YAML 头），Codex 对应的 skill 需要**加 YAML 头部** + 相同正文

**同步规则**：
- 新增/修改/删除 Claude command 时，必须同步 `.codex/skills/` 下对应的 skill
- Codex 的 command 类 skill 必须有 YAML 头部（name + description + 触发场景 + 触发词）

---

## 前置条件

在创建新技能前，请确保：

- [ ] **已了解双进程架构**：理解 WebView（React）↔ IPC ↔ Rust Core 的通信模式
- [ ] **已了解后端三层架构**：Commands → Services → Database 的职责划分
- [ ] **已了解 `src-tauri/src/` 结构**：`commands/`、`services/`、`database/`、`models/`、`error.rs`、`state.rs`
- [ ] **已了解 `src/` 前端结构**：`pages/`、`components/`、`lib/api/`、`store/`、`types/`
- [ ] **已了解技能系统**：技能如何被触发、声明、评估和激活
- [ ] **已读现有技能**：至少阅读过 3 个现有技能（如 `tauri-commands`、`tauri-events`、`ui-frontend`）
- [ ] **已明确技能范围**：技能应该解决什么问题，涵盖哪些触发词
- [ ] **已找到参考代码**：该技能对应的项目中的参考代码或最佳实践

---

## YAML 头部强制规范（最高优先级）

> **警告**：这是创建技能时最容易出错的部分！必须严格遵守以下规范。

### 强制格式

每个 SKILL.md 文件**必须**以 YAML 头部开始，格式如下：

```yaml
---
name: {技能名称}
description: |
  {第一行：简短描述（一句话说明技能用途）}

  触发场景：
  - {场景1}
  - {场景2}
  - {场景3}
  （至少3个场景）

  触发词：{关键词1}、{关键词2}、{关键词3}、{关键词4}
  （至少5个触发词，用中文顿号或斜杠分隔）
---
```

### name 字段规范

| 规则 | 说明 | 示例 |
|------|------|------|
| **格式** | kebab-case（全小写，横线连接） | `tauri-commands` |
| **禁止** | 下划线、驼峰、空格 | `tauri_commands`, `tauriCommands` |
| **长度** | 2-4 个单词 | `tauri-events`, `rust-fundamentals` |
| **语义** | 清晰表达技能领域 | `tauri-plugins`, 而非 `plugins`（太宽泛） |

### description 字段规范

| 部分 | 要求 | 示例 |
|------|------|------|
| **第一行** | 一句话说明技能用途（以"当需要..."或"用于..."开头） | `当需要开发 Tauri Command 时自动使用此 Skill。` |
| **触发场景** | 至少 3 个具体场景，每个场景一行 | `- 需要实现进度回报模式`<br>`- 需要 Command 中注入 State` |
| **触发词** | 至少 5 个关键词，用顿号或斜杠分隔 | `Command、invoke、#[tauri::command]、IPC、异步Command` |
| **空行** | 各部分之间必须有空行 | 第一行后空一行，触发场景后空一行 |

### 正确示例

```yaml
---
name: tauri-commands
description: |
  Tauri Command 高级开发技能，覆盖异步 Command、状态注入、流式传输、事件通知等高级模式。

  触发场景：
  - 需要开发复杂的 Tauri Command
  - 需要 Command 中访问 AppHandle/Window
  - 需要实现进度回报/流式数据
  - 需要 Command 之间共享逻辑

  触发词：Command、tauri::command、invoke、高级Command、async command、进度、stream
---
```

### 常见错误示例

**错误 1：name 使用下划线或驼峰**
```yaml
---
name: tauri_commands  # 应该用横线：tauri-commands
description: |
  ...
---
```

**错误 2：description 过于简短**
```yaml
---
name: tauri-commands
description: |
  Tauri Command 开发  # 缺少触发场景和触发词
---
```

**错误 3：触发词太少**
```yaml
---
name: tauri-commands
description: |
  当需要开发 Tauri Command 时使用。

  触发词：Command、invoke  # 只有2个，至少需要5个
---
```

**错误 4：缺少必要的空行**
```yaml
---
name: tauri-commands
description: |
  当需要开发 Tauri Command 时使用。
  触发场景：  # 第一行后应该空一行
  - 场景1
  - 场景2
  触发词：...  # 触发场景后应该空一行
---
```

**错误 5：触发场景不具体**
```yaml
---
name: tauri-commands
description: |
  当需要开发 Tauri Command 时使用。

  触发场景：
  - Command 开发  # 太宽泛，应该具体说明：如"需要实现进度回报模式"
  - 数据处理  # 太宽泛，应该具体说明：如"需要 Command 中注入 State"
---
```

### YAML 头部验证清单

创建 YAML 头部后，必须通过以下所有检查：

- [ ] `name` 使用 kebab-case 格式（全小写+横线）
- [ ] `name` 长度为 2-4 个单词
- [ ] `name` 语义清晰，不过于宽泛
- [ ] `description` 第一行是完整的一句话说明
- [ ] 第一行以"当需要..."或"用于..."开头
- [ ] 第一行后有空行
- [ ] 包含"触发场景："标题
- [ ] 至少有 3 个具体的触发场景
- [ ] 每个触发场景都具体明确（不是宽泛描述）
- [ ] 触发场景后有空行
- [ ] 包含"触发词："标题
- [ ] 至少有 5 个触发词
- [ ] 触发词用中文顿号（、）或斜杠（/）分隔
- [ ] 触发词包含技术术语和常用表达
- [ ] YAML 头部以 `---` 开始和结束

---

## 第 1 步：分析与规划（规划阶段）

### 1.1 定义技能属性

在创建 SKILL.md 前，先回答以下问题：

**技能名称**（kebab-case）：
```
示例：tauri-commands, tauri-events, rust-fundamentals, security-permissions
规则：全小写，单词用横线连接，不包含下划线
```

**技能描述**（技能触发的核心关键词）：
```
示例：
描述：当需要开发 Tauri Command、实现 IPC 通信时自动使用此 Skill。

触发场景：
- 需要创建新的 Rust Command
- 需要实现异步 Command + 进度回报

触发词：Command、invoke、#[tauri::command]、IPC、async command
```

**技能类别**（技术领域）：
```
Rust 后端：Command、Service、Database、Plugin、错误处理、状态管理
React 前端：Ant Design 组件、Zustand 状态、React Router、TailwindCSS
Tauri 特性：Capabilities、Events、Window 管理、打包分发、自动更新
跨领域：架构设计、IPC 通信、性能优化、测试、国际化
```

**关联参考代码**（项目中的真实例子）：
```
示例：
- src-tauri/src/commands/config.rs (Rust Command 示例)
- src-tauri/src/services/config.rs (Service 层示例)
- src-tauri/src/database/mod.rs (Database 层示例)
- src/pages/settings/index.tsx (React 页面示例)
- src/lib/api/index.ts (API 封装示例)
```

### 1.2 分析覆盖范围

```
核心知识点：
- 你需要文档化哪些 Rust 结构体、方法、宏？
- 你需要文档化哪些 React 组件、Hook、API？
- 需要包含多少个代码示例？
- 覆盖多少个使用场景？

文档量估计：
- 小型技能（工具类）：200-300 行
- 中型技能（功能模块）：400-600 行
- 大型技能（完整流程）：600+ 行

参考数据：
- tauri-commands: 655 行（大型）
- tauri-events: ~400 行（中型）
- rust-fundamentals: ~500 行（中型）
```

---

## 第 2 步：编写 SKILL.md（实现阶段）

### 2.1 文件结构模板

```markdown
---
name: {技能名称}
description: |
  {详细描述，包括触发场景和触发词}
---

# {技能标题} 指南

## 概述
{简明介绍，1-2 段}

## 核心工具类/API
{主要 Rust 结构体、函数、宏 或 React 组件、Hook 列表}

## 使用规范
{最佳实践和规则}

## 常见错误与最佳实践
{正确做法 vs 错误做法对比}

## 实战示例
{3-5 个真实代码例子}

## 常见问题
{FAQ}
```

### 2.2 编写清单

**必须按顺序完成，YAML 头部是第一优先级！**

#### 第一优先级：YAML 头部（必须最先完成）

- [ ] **name 字段**：使用 kebab-case 格式（全小写+横线）
- [ ] **name 字段**：长度为 2-4 个单词，语义清晰
- [ ] **description 第一行**：完整的一句话说明，以"当需要..."或"用于..."开头
- [ ] **description 第一行后**：有空行
- [ ] **触发场景**：至少 3 个具体场景（不是宽泛描述）
- [ ] **触发场景后**：有空行
- [ ] **触发词**：至少 5 个关键词，用顿号或斜杠分隔
- [ ] **YAML 格式**：以 `---` 开始和结束

#### 第二优先级：核心内容

- [ ] **概述部分**：简明扼要说明技能的作用（150-200 字）
- [ ] **核心内容**：包含 3+ 个主要技术点
- [ ] **代码示例**：至少 5 个真实或接近真实的代码片段（Rust + TypeScript/React）
- [ ] **错误对比**：列举 3+ 个常见错误及其正确做法
- [ ] **参考代码**：附带项目中的具体代码位置
- [ ] **复杂性适中**：避免过于基础或过于深入

### 2.3 推荐的内容结构

#### Rust 后端技能示例（Command / Service / Database 等）

```markdown
## 核心 Rust 结构体/函数
{列表：struct、fn、macro}

## 关键规范
{表格：项目、规范}

## 标准代码模板
### Command 层（src-tauri/src/commands/）
### Service 层（src-tauri/src/services/）
### Database 层（src-tauri/src/database/）
### Model（src-tauri/src/models/）

## 后端使用示例
{3-5 个真实场景}

## 前端调用注意事项
{invoke 调用、类型对齐、错误处理}

## 常见错误
### 正确做法
### 常见错误
```

**Rust 后端技能代码示例**：

```rust
// ─── Command 层（薄 IPC 包装）───
// src-tauri/src/commands/user.rs
use crate::services::user::UserService;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn list_users(state: State<'_, AppState>) -> Result<Vec<User>, String> {
    UserService::list(&state.db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_user(
    state: State<'_, AppState>,
    name: String,
    email: String,
) -> Result<User, String> {
    if name.is_empty() {
        return Err("名称不能为空".into());
    }
    UserService::create(&state.db, &name, &email).map_err(|e| e.to_string())
}
```

```rust
// ─── Service 层（业务逻辑）───
// src-tauri/src/services/user.rs
use crate::database::Database;
use crate::error::AppError;
use crate::models::User;

pub struct UserService;

impl UserService {
    pub fn list(db: &Database) -> Result<Vec<User>, AppError> {
        db.list_users()
    }

    pub fn create(db: &Database, name: &str, email: &str) -> Result<User, AppError> {
        // 业务验证...
        db.insert_user(name, email)
    }
}
```

```rust
// ─── Database 层（数据访问）───
// src-tauri/src/database/mod.rs
impl Database {
    pub fn list_users(&self) -> Result<Vec<User>, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT id, name, email FROM users ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(User {
                id: row.get(0)?,
                name: row.get(1)?,
                email: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }
}
```

#### React 前端技能示例（UI 组件 / 状态 / 页面等）

```markdown
## 组件库概述
{可用的 Ant Design 组件 + TailwindCSS 类}

## 强制使用规则
{表格：场景、必用组件、禁用做法}

## TypeScript 类型定义规范
{类型定义和 API 调用示例}

## 实战示例
### 场景 1：列表页面（Table + Card）
### 场景 2：表单页面（Form + Modal）
### 场景 3：详情页面（Descriptions）

## 常见错误
### 使用原生 HTML 而非 Ant Design
### 不使用 @/ 路径别名
### 裸写 invoke() 不封装到 lib/api/
```

**React 前端技能代码示例**：

```typescript
// ─── API 封装层 ───
// src/lib/api/index.ts
import { invoke } from "@tauri-apps/api/core";

export const userApi = {
  list: () => invoke<User[]>("list_users"),
  create: (name: string, email: string) =>
    invoke<User>("create_user", { name, email }),
};
```

```tsx
// ─── React 页面组件 ───
// src/pages/users/index.tsx
import { Card, Table, Button, Form, Input, Modal, message } from "antd";
import { userApi } from "@/lib/api";
import type { User } from "@/types";

export default function UsersPage() {
  const [data, setData] = useState<User[]>([]);
  const [loading, setLoading] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [form] = Form.useForm();

  async function loadData() {
    setLoading(true);
    try {
      const users = await userApi.list();
      setData(users);
    } catch (e) {
      message.error(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { loadData(); }, []);

  async function handleCreate(values: { name: string; email: string }) {
    try {
      await userApi.create(values.name, values.email);
      message.success("创建成功");
      setModalOpen(false);
      form.resetFields();
      loadData();
    } catch (e) {
      message.error(String(e));
    }
  }

  return (
    <div className="max-w-4xl mx-auto">
      <Card
        title="用户管理"
        extra={<Button type="primary" onClick={() => setModalOpen(true)}>新增</Button>}
      >
        <Table dataSource={data} loading={loading} rowKey="id" columns={[
          { title: "ID", dataIndex: "id" },
          { title: "名称", dataIndex: "name" },
          { title: "邮箱", dataIndex: "email" },
        ]} />
      </Card>
      <Modal title="新增用户" open={modalOpen} onCancel={() => setModalOpen(false)} onOk={() => form.submit()}>
        <Form form={form} onFinish={handleCreate} layout="vertical">
          <Form.Item name="name" label="名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="email" label="邮箱" rules={[{ required: true, type: "email" }]}>
            <Input />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
```

---

## 第 3 步：双系统声明（声明阶段）

> **注意区分**：这一步针对的是**技能（Skill）**的声明。如果你是在创建**命令（Command）**，请参考下方的"3.3 Command 的声明方式"。

### 3.1 在 Hook 中声明（`.claude/hooks/skill-forced-eval.cjs`）

**位置**：技能列表区域

**格式**：
```javascript
- {技能名称}: {触发词，用空格或中文逗号分隔}
```

**示例**：
```javascript
- tauri-commands: Command、tauri::command、invoke、高级Command、async command、进度
- tauri-events: 事件、emit、listen、Event、EventTarget、on
```

**修改步骤**：
1. 打开 `.claude/hooks/skill-forced-eval.cjs`
2. 在技能列表中找到合适的插入位置（按字母序或逻辑分组）
3. 添加新一行：`- {技能名}: {触发词}`
4. 保存文件

### 3.2 在 AGENTS.md 中声明

**位置**：`AGENTS.md` 的技能清单表格

**格式**：
```markdown
| \`{技能名}\` | {触发条件（description）} |
```

**示例**：
```markdown
| `add-skill` | 为框架添加新技能、编写技能文档 |
| `tauri-commands` | Command、tauri::command、invoke、高级Command、async command、进度 |
```

**修改步骤**：
1. 打开 `AGENTS.md`
2. 找到技能清单表格
3. 在合适位置添加新行
4. 保存文件

### 3.3 验证声明

```bash
# 检查 hook 文件
grep "add-skill" .claude/hooks/skill-forced-eval.cjs

# 检查 AGENTS.md
grep "add-skill" AGENTS.md
```

### 3.4 Command 的声明（仅适用于斜杠命令）

新增 Claude command 时，除了创建 `.claude/commands/xxx.md`，还需要：
1. 在 `.codex/skills/xxx/SKILL.md` 创建对应 skill（加 YAML 头部 + 相同正文）
2. 在 Hook 文件和 AGENTS.md 中声明（同 3.1/3.2）

---

## 第 4 步：Codex 系统同步（同步阶段）

### 4.1 复制到 Codex 目录

由于框架支持 Claude Code 和 Codex 两个系统，需要保持技能同步。

**步骤**：

1. **创建目录**：
   ```
   .codex/skills/[技能名]/
   ```

2. **复制文件**：
   将 `.claude/skills/[技能名]/SKILL.md` 复制到 `.codex/skills/[技能名]/SKILL.md`

3. **验证一致性**：
   确保两个文件内容完全相同

**示例**（tauri-commands）：
```
.claude/skills/tauri-commands/SKILL.md (655 行)
.codex/skills/tauri-commands/SKILL.md  (655 行，完全相同)
```

### 4.2 Command 同步到 Codex

新增/修改 Claude command 时，同步 `.codex/skills/[command名]/SKILL.md`（加 YAML 头部 + 相同正文）。

### 4.3 检查清单

- [ ] `.codex/skills/` 下对应目录已创建
- [ ] Skill：两端文件内容完全相同
- [ ] Command：Codex 文件包含 YAML 头部 + 与 Claude command 一致的正文

---

## 第 5 步：验证与测试（验证阶段）

### 5.1 完整检查清单

运行以下检查确保技能正确添加：

**文件检查**：
```bash
# 检查 Claude Code 文件存在
ls -la .claude/skills/[技能名]/SKILL.md

# 检查 Codex 文件存在
ls -la .codex/skills/[技能名]/SKILL.md

# 验证文件大小接近（应该完全相同）
wc -l .claude/skills/[技能名]/SKILL.md
wc -l .codex/skills/[技能名]/SKILL.md
```

**声明检查**：
```bash
# 检查 hook 声明（注意扩展名为 .cjs）
grep -n "[技能名]:" .claude/hooks/skill-forced-eval.cjs

# 检查 AGENTS.md 声明
grep -n "[技能名]" AGENTS.md
```

**内容检查**：
- [ ] **YAML 头部格式正确**（`name:` 使用 kebab-case，`description:` 包含触发场景和触发词）
- [ ] **YAML 头部完整**（至少 3 个触发场景，至少 5 个触发词）
- [ ] **触发场景具体明确**（不是宽泛描述）
- [ ] **触发词包含技术术语**（如 `#[tauri::command]`、`invoke`、`State<T>` 等）
- [ ] 技能描述包含 3+ 个触发场景
- [ ] 至少包含 5 个代码示例（Rust + TypeScript/React）
- [ ] 至少包含 3 个错误对比
- [ ] 包含真实项目代码参考
- [ ] 没有语法错误或格式问题

### 5.2 激活测试

在实际使用中，验证技能是否正确激活：

1. **编写包含触发词的提问**：
   ```
   用户提问："我需要添加一个新的文件管理技能"
   ```

2. **验证技能评估**：
   Hook 应该输出：
   ```
   ## 强制技能激活流程

   ### 步骤 1 - 评估（必须在响应中明确展示）

   匹配技能：
   - add-skill: 涉及新技能开发
   ```

3. **验证激活**：
   应该看到 `Skill(add-skill)` 被调用

---

## 实战案例：tauri-commands 技能

下面以实际添加 `tauri-commands` 技能为例，展示完整流程：

### 步骤 1：分析与规划

**技能属性**：
```
名称：tauri-commands
类别：Rust 后端核心技能
范围：Tauri Command 开发、IPC 通信、异步调用、进度回报、状态注入
参考模块：src-tauri/src/commands/
核心 API：#[tauri::command]、invoke、State<T>、AppHandle、Window、Emitter
```

### 步骤 2：编写 SKILL.md

创建文件：`.claude/skills/tauri-commands/SKILL.md`

包含以下部分：
- 概述（Tauri Command 开发模式）
- 模块化组织（三层架构：Command → Service → Database）
- Command 注入参数（AppHandle、Window、State、组合注入）
- 异步 Command 模式
- 进度回报模式（后端 emit + 前端 listen）
- 子进程处理（Windows 防弹窗 CREATE_NO_WINDOW）
- 错误处理最佳实践
- 批量操作模式
- 参数验证模式
- 完整实战示例（文件批量处理）

**最终行数**：655 行

### 步骤 3：声明技能

**在 hook 中添加**（`.claude/hooks/skill-forced-eval.cjs`）：
```javascript
- tauri-commands: Command、tauri::command、invoke、高级Command、async command、进度、stream
```

**在 AGENTS.md 中添加**：
```markdown
| `tauri-commands` | Command、tauri::command、invoke、高级Command、async command、进度、stream |
```

### 步骤 4：Codex 同步

复制文件：
```
.codex/skills/tauri-commands/SKILL.md  (655 行，与 .claude 完全相同)
```

### 步骤 5：验证

所有检查通过：
- 文件存在于两个系统
- Hook 和 AGENTS.md 均已声明
- 内容符合规范
- 可被正确激活

---

## 修改现有技能（维护流程）

> **核心原则**：`.claude/skills/` 是主目录（source of truth），`.codex/skills/` 是镜像。
> 所有修改都在 `.claude/skills/` 中进行，然后同步到 `.codex/skills/`。

### 场景 1：修改技能内容（最常见）

当需要修改现有技能的文档内容（如新增规则、修正示例、补充场景）：

**步骤**：

1. **在 `.claude/skills/[技能名]/SKILL.md` 中修改内容**
2. **同步到 Codex**：
   ```bash
   cp .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md
   ```
3. **验证一致性**：
   ```bash
   diff .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md
   ```
   输出应为空（无差异）

### 场景 2：修改触发词或触发场景

当需要修改技能的 YAML 头部（触发词、触发场景、描述）：

**步骤**：

1. **修改 SKILL.md 的 YAML 头部**
2. **同步修改 Hook 文件**（`.claude/hooks/skill-forced-eval.cjs`）中对应的触发词行
3. **同步修改 AGENTS.md** 中对应的技能描述行
4. **复制到 Codex**：
   ```bash
   cp .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md
   ```
5. **验证三处一致**：
   ```bash
   # 验证文件同步
   diff .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md
   # 验证 Hook 声明
   grep "[技能名]" .claude/hooks/skill-forced-eval.cjs
   # 验证 AGENTS.md 声明
   grep "[技能名]" AGENTS.md
   ```

### 场景 3：重命名技能

当需要重命名技能（如 `old-name` -> `new-name`）：

**步骤**：

1. **创建新目录并移动文件**：
   ```bash
   # Claude Code
   mkdir -p .claude/skills/[新名称]
   mv .claude/skills/[旧名称]/SKILL.md .claude/skills/[新名称]/SKILL.md
   rmdir .claude/skills/[旧名称]

   # Codex
   mkdir -p .codex/skills/[新名称]
   cp .claude/skills/[新名称]/SKILL.md .codex/skills/[新名称]/SKILL.md
   rm -rf .codex/skills/[旧名称]
   ```
2. **修改 SKILL.md 中 YAML 头部的 `name` 字段**
3. **修改 Hook 文件**（`.claude/hooks/skill-forced-eval.cjs`）中的技能名
4. **修改 AGENTS.md** 中的技能名
5. **全局搜索旧名称**，确保无遗漏引用：
   ```bash
   grep -r "[旧名称]" .claude/ .codex/ AGENTS.md CLAUDE.md
   ```

### 场景 4：删除技能

当需要删除不再需要的技能：

**步骤**：

1. **删除文件**：
   ```bash
   rm -rf .claude/skills/[技能名]
   rm -rf .codex/skills/[技能名]
   ```
2. **从 Hook 文件中移除**对应的触发词行
3. **从 AGENTS.md 中移除**对应的技能行
4. **全局搜索确认无遗漏引用**

### 场景 5：修改 Claude Command

修改 `.claude/commands/xxx.md` 后，同步正文到 `.codex/skills/xxx/SKILL.md`（保留其 YAML 头部）。

### 修改后的检查清单

每次修改现有技能后，必须通过以下检查：

- [ ] `.claude/skills/[技能名]/SKILL.md` 已修改
- [ ] `.codex/skills/[技能名]/SKILL.md` 已同步（`diff` 无差异）
- [ ] 如果修改了触发词：Hook 文件和 AGENTS.md 已同步更新
- [ ] 如果重命名/删除：旧名称已全局搜索确认无遗漏

---

## 常见陷阱与解决方案

### 陷阱 0：YAML 头部格式错误（最常见！）

**症状**：技能无法被正确识别或激活，Hook 评估时找不到该技能

**原因**：
- `name` 使用了下划线或驼峰命名
- `description` 过于简短，缺少触发场景或触发词
- 触发词数量不足（少于 5 个）
- 缺少必要的空行
- 触发场景描述过于宽泛

**解决**：
1. 严格按照"YAML 头部强制规范"章节的要求编写
2. 使用本文档提供的正确示例作为模板
3. 完成"YAML 头部验证清单"中的所有检查项
4. 参考现有技能（如 `tauri-commands`）的 YAML 头部格式

**正确示例**：
```yaml
---
name: tauri-plugins
description: |
  当需要开发、集成或配置 Tauri 插件时自动使用此 Skill。

  触发场景：
  - 需要集成 Tauri 官方插件（store/log/dialog/fs）
  - 需要开发自定义 Tauri 插件
  - 需要在 Capabilities 中声明插件权限
  - 需要在 lib.rs 中注册插件

  触发词：Plugin、tauri-plugin、插件、plugin、store、log、dialog、fs、notification、权限声明
---
```

### 陷阱 1：新增 Command 时忘记在 Codex 创建对应 Skill

**解决**：在 `.codex/skills/[command名]/SKILL.md` 中创建带 YAML 头部的对应文件

### 陷阱 2：Hook 文件扩展名错误

**症状**：修改了 `.js` 文件但实际使用的是 `.cjs`，导致技能声明无效

**原因**：Tauri 框架的 Hook 文件使用 `.cjs` 扩展名（CommonJS 模块）

**解决**：
- 始终修改 `.claude/hooks/skill-forced-eval.cjs`（注意 `.cjs` 扩展名）
- 同理，`pre-tool-use.cjs` 也是 `.cjs` 扩展名

### 陷阱 3：忘记复制到 Codex 目录

**解决**：`cp -r .claude/skills/[技能名] .codex/skills/`

### 陷阱 4：触发词设置过于宽泛

**症状**：技能被过度激活，在不相关的场景中被触发

**原因**：触发词选择不当，例如使用"开发"而不是更具体的"Command 开发"

**解决**：
- 使用具体、专业的触发词
- 避免过于通用的词汇
- 参考现有技能的触发词风格
- Rust 侧使用专有术语如 `#[tauri::command]`、`State<T>`、`AppHandle`
- React 侧使用具体组件名如 `Ant Design Table`、`useCommand`、`Zustand store`

### 陷阱 5：文档内容过于冗长或过于简短

**症状**：技能无法提供实际帮助

**原因**：文档要么内容不足，要么冗长无焦点

**解决**：
- 瞄准 400-600 行的中等规模
- 包含 5+ 个真实代码示例（Rust 和 TypeScript/React 各至少 2 个）
- 明确区分"最佳实践"vs"常见错误"
- 后端示例遵循三层架构（Command → Service → Database）

### 陷阱 6：技能覆盖范围与现有技能重叠

**症状**：多个技能处理同一问题，造成混淆

**原因**：未充分检查现有技能列表

**解决**：
- 在创建前阅读现有技能的 description
- 与相关技能进行边界划分
- 必要时在文档中说明与其他技能的关系

**已有技能列表参考**：
| 技能 | 覆盖范围 |
|------|---------|
| `tauri-commands` | Rust Command 开发（三层架构、注入、异步） |
| `tauri-events` | 事件通信（emit/listen/双向） |
| `tauri-plugins` | 插件集成与开发 |
| `tauri-capabilities` | Capabilities 权限声明 |
| `tauri-window-management` | 窗口管理（创建/操作/多窗口） |
| `tauri-packaging` | 打包分发 |
| `tauri-updater` | 自动更新 |
| `rust-fundamentals` | Rust 基础（所有权/生命周期/错误处理） |
| `security-permissions` | 安全权限模型 |
| `ui-frontend` | React 前端开发（Ant Design + TailwindCSS） |
| `store-management` | 状态管理（Zustand） |
| `database-ops` | 数据库操作（rusqlite） |
| `error-handler` | 错误处理（AppError + ErrorBoundary） |
| `project-navigator` | 项目结构导航 |
| `project-init` | 项目初始化 |
| `api-development` | API 封装（invoke 调用层） |

### 陷阱 7：代码示例混用框架概念

**症状**：Rust 后端示例中出现 Web 框架概念（REST API、HTTP 路由等）

**原因**：将 Tauri 桌面应用与 Web 应用混淆

**解决**：
- Tauri 没有 HTTP 路由，使用 `#[tauri::command]` + `invoke()`
- 没有 REST API 概念，使用 IPC Command 模式
- 数据库是本地 SQLite，不是远程数据库服务
- 前端通过 `invoke()` 调用 Rust，不是 `fetch()` 调用 API

---

## 技能开发清单（最终版）

在提交新技能前，请确认以下所有项目：

### 第一优先级：YAML 头部（必须最先检查）

- [ ] `name` 使用 kebab-case 格式（全小写+横线）
- [ ] `name` 长度为 2-4 个单词
- [ ] `name` 语义清晰，不过于宽泛
- [ ] `description` 第一行是完整的一句话说明
- [ ] 第一行以"当需要..."或"用于..."开头
- [ ] 第一行后有空行
- [ ] 包含"触发场景："标题
- [ ] 至少有 3 个具体的触发场景
- [ ] 每个触发场景都具体明确（不是宽泛描述）
- [ ] 触发场景后有空行
- [ ] 包含"触发词："标题
- [ ] 至少有 5 个触发词
- [ ] 触发词用中文顿号（、）或斜杠（/）分隔
- [ ] 触发词包含技术术语和常用表达
- [ ] YAML 头部以 `---` 开始和结束

### 规划阶段
- [ ] 技能名称已确定（kebab-case）
- [ ] 触发词列表已确定（5+ 个）
- [ ] 覆盖范围已明确（不与现有技能重叠）
- [ ] 参考代码已找到（`src-tauri/src/` 或 `src/`）

### 实现阶段
- [ ] SKILL.md 已创建在 `.claude/skills/`
- [ ] YAML 头部格式正确
- [ ] 文档包含 5+ 代码示例（Rust + TypeScript/React）
- [ ] 文档包含 3+ 错误对比
- [ ] 文档长度 400-600+ 行
- [ ] 所有代码片段都经过验证
- [ ] 后端示例遵循三层架构
- [ ] 前端示例使用 Ant Design + TailwindCSS + @/ 路径别名

### 声明阶段
- [ ] Hook 文件已更新（`.claude/hooks/skill-forced-eval.cjs`）
- [ ] AGENTS.md 已更新（技能表格）
- [ ] 两处声明的触发词一致

### 同步阶段
- [ ] 文件已复制到 `.codex/skills/`
- [ ] 两个系统的文件内容完全相同
- [ ] 文件行数验证无误

### 验证阶段
- [ ] 文件检查通过（存在且完整）
- [ ] 声明检查通过（Hook `.cjs` 文件和 AGENTS.md）
- [ ] 内容检查通过（格式、完整性）
- [ ] 激活测试通过（能被正确识别和调用）

### 维护阶段（修改现有技能时）
- [ ] 修改在 `.claude/skills/` 主目录中完成
- [ ] 已同步到 `.codex/skills/`（`diff` 无差异）
- [ ] 如修改触发词：Hook 文件（`.cjs`）和 AGENTS.md 已更新
- [ ] 如重命名/删除：旧名称已全局搜索确认无遗漏

---

## 快速参考

### 快速创建命令

```bash
# 1. 创建 Claude Code 目录和文件
mkdir -p .claude/skills/[技能名]
touch .claude/skills/[技能名]/SKILL.md

# 2. 复制到 Codex（创建文件后执行）
mkdir -p .codex/skills/[技能名]
cp .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md

# 3. 验证双系统一致性
diff .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md

# 4. 确认声明
grep "[技能名]" .claude/hooks/skill-forced-eval.cjs
grep "[技能名]" AGENTS.md
```

### 快速同步命令（修改现有技能后）

```bash
# 1. 同步到 Codex
cp .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md

# 2. 验证一致性（输出应为空）
diff .claude/skills/[技能名]/SKILL.md .codex/skills/[技能名]/SKILL.md
```

### 文件大小参考

| 技能类型 | 预期行数 | 示例 |
|---------|---------|------|
| 小型（工具类） | 200-300 | utils-toolkit, json-serialization |
| 中型（功能模块） | 400-600 | tauri-events, rust-fundamentals |
| 大型（完整流程） | 600+ | tauri-commands, ui-frontend |

---

## 下一步

技能创建完成后：

1. **集成到项目**：将技能文件提交到项目仓库
2. **收集反馈**：在实际使用中优化和完善技能文档
3. **维护更新**：随着 Tauri / React / Rust 版本升级，定期更新技能文档
4. **检查覆盖**：确保新增模块都有对应技能支持

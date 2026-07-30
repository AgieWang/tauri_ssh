# 通用密码库、多端同步与安全共享功能详细规划

**状态**: 需求规划稿
**创建时间**: 2026-07-11
**目标版本**: v0.1 本地密码库，v0.2 浏览器与个人同步，v0.3 安全共享
**目标模块**: 安全凭证 / 通用密码库 / 系统密钥库 / 浏览器扩展 / 移动端 / 同步服务
**关联方案**: `docs/requirements/2026-06-27-secure-credential-vault-plan.md`
**参考资料**:
- Chrome Native Messaging: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging
- Chrome Extensions API Reference: https://developer.chrome.com/docs/extensions/reference/api
- Bitwarden Security Whitepaper: https://bitwarden.com/help/bitwarden-security-white-paper/

---

## 1. 背景与目标

Tauri SSH 已具备安全凭证、AES-256-GCM 加密保存、短期会话、Provider Adapter、审批、审计和 MCP 受控调用能力，但现有安全凭证主要面向 Git、Jenkins、HTTP API、数据库和 SSH 等机器或运维场景，并不是完整的个人密码管理器。

本需求在 `安全凭证` 一级模块中新增 `通用密码库`，形成类似 Bitwarden 的本地优先密码管理能力：

- 管理网站、应用和基础认证账号。
- 生成、保存、搜索、复制、自动填充密码。
- 使用主密码、设备密钥和系统生物识别解锁。
- 将加密后的密码库同步到多个桌面端、移动端和浏览器扩展。
- 支持个人共享、组织、集合和成员权限。
- 与现有安全凭证建立受控引用，但不让 MCP 或 AI 默认读取个人密码。

一句话定位：

> 一个本地优先、端到端加密、服务端零知识的通用密码库，通过系统密钥库完成设备解锁，通过自有浏览器扩展完成保存和自动填充，并通过密文同步与集合密钥实现多端统一管理和安全共享。

---

## 2. 产品边界

### 2.1 通用密码库与现有安全凭证的关系

| 模块 | 主要对象 | 默认调用者 | 明文策略 | MCP 策略 |
|------|----------|------------|----------|----------|
| 通用密码库 | 网站登录、应用账号、安全笔记、身份、银行卡 | 本地用户、浏览器扩展、移动端 | 解锁后按需显示或填充 | 默认完全禁止 |
| 安全凭证 | Token、API Key、SSH/DB/Jenkins/Git 凭证 | Rust Service、受控 MCP 工具 | 前端和 MCP 永不回显 | 按 scope、审批和审计代理使用 |

两者复用：

- 加密基础设施。
- 系统密钥库适配器。
- 审计与敏感字段脱敏。
- 设备管理、同步协议和安全策略。

两者隔离：

- 通用密码库有主密码、锁定状态、显示密码、复制和自动填充流程。
- 安全凭证继续保持“前端和 MCP 不读取明文”的现有契约。
- 个人密码不能因为绑定了 `credentialKey` 就自动暴露给 MCP。
- 从通用密码条目创建安全凭证引用必须由用户明确操作，并生成独立权限和审计记录。

### 2.2 系统密钥库边界

系统密钥库集成范围：

- macOS Keychain。
- Windows Credential Manager / DPAPI 保护的应用秘密。
- Linux Secret Service，缺失时提示降级而不是静默明文保存。

系统密钥库只保存 Tauri SSH 自己命名空间内的设备解锁材料，例如：

```text
service = com.agilefr.tauri_ssh.password_vault
account = vault/<vault_id>/device/<device_id>
value   = 本设备包装后的 device_unlock_key
```

禁止：

- 静默枚举或导入用户系统中的全部密码。
- 把主密码直接写入系统密钥库。
- 把未加密的完整密码库放入系统密钥库。
- 系统密钥库不可用时自动降级到 SQLite 明文种子。

如需导入其它应用写入系统密钥库的凭据，必须由用户显式选择来源、逐项确认，并受平台 API 能力限制。

### 2.3 浏览器密钥库边界

Chrome / Edge / Chromium 的公开扩展 API 不提供通用的内建密码库读取和写入能力。规划采用以下口径：

- `浏览器统一管理` 指 Tauri SSH 自有浏览器扩展的密码保存、搜索、生成和自动填充。
- 扩展通过 Native Messaging 或同步 API 使用 Tauri SSH 密码库。
- 不直接修改 Chrome、Edge、Firefox 的内部密码数据库。
- 从浏览器原生密码库迁移采用用户主动导出的 CSV/JSON 文件导入。
- 向浏览器原生密码库迁移采用用户主动导出，不承诺后台直接写入。
- 浏览器数据目录、Cookies、Session、内建密码数据库都不作为读写接口。

---

## 3. 核心安全原则

1. **零知识同步**
   - 所有密码条目在客户端加密后上传。
   - 同步服务不能获得主密码、Vault Key、Item Key 或条目明文。

2. **分层密钥**
   - 主密码只用于派生 Master Key。
   - Master Key 只用于包装 Vault Key，不直接批量加密业务条目。
   - 每个条目使用独立 Item Key，便于共享、轮换和撤销。

3. **设备隔离**
   - 每个设备生成独立设备密钥对和 Device Unlock Key。
   - 系统密钥库泄露只影响当前设备，不等于主密码泄露。

4. **最小暴露**
   - 列表默认不返回密码、TOTP Secret、银行卡号或安全笔记正文。
   - 显示、复制、填充和导出分别授权、分别审计。

5. **本地优先**
   - 离线可创建、编辑、搜索和使用密码。
   - 联网后以密文变更日志同步。

6. **共享不复制明文**
   - 通过 Collection Key 和成员公钥封装共享密钥。
   - 不通过聊天、链接参数、日志或数据库明文复制密码。

7. **MCP 默认不可见**
   - 不提供 `password_list`、`password_read`、`password_export` 等 MCP 工具。
   - AI 不能借用安全凭证会话读取个人密码库。

---

## 4. 推荐总体架构

```text
┌─────────────────── 客户端可信边界 ───────────────────┐
│                                                      │
│  React Desktop UI ── invoke ── Rust Vault Service    │
│                              │                       │
│                              ├─ Crypto Service       │
│                              ├─ System Keychain      │
│                              ├─ Local SQLite         │
│                              ├─ Sync Client          │
│                              └─ Browser Bridge       │
│                                                      │
│  Browser Extension ─ Native Messaging / Sync API ───┤
│  Mobile Tauri Client ─ Local Vault + Sync Client ───┤
└──────────────────────────────────────────────────────┘
                         │ 仅密文、版本和设备公钥
                         ▼
┌────────────────── 零知识同步服务 ────────────────────┐
│  Account / Device / Revision / Ciphertext / Sharing  │
│  不持有主密码、Vault Key、Item Key 和条目明文          │
└──────────────────────────────────────────────────────┘
```

### 4.1 为什么不直接复用远程访问网关

现有 `src-tauri/src/remote` 仅具备 Token 鉴权、HTTP/WebSocket 骨架，并要求桌面端在线。它可以用于 v0.1 局域网验证，但不能作为最终多端同步方案：

- 桌面端关闭后其它设备无法同步。
- 缺少账号、设备、公钥、密文版本、冲突和撤销模型。
- 远程网关当前是设备控制入口，不应与密码库同步服务共用长期 Token。

推荐新增独立、可自托管的零知识同步服务；桌面端、移动端和浏览器扩展都作为对等客户端。

### 4.2 同步方案对比

| 方案 | 优点 | 缺点 | 结论 |
|------|------|------|------|
| 桌面端作为同步中心 | 开发量小，可复用 remote gateway | 必须保持桌面在线，分享和撤销弱 | 仅用于原型 |
| WebDAV/S3 加密文件 | 部署简单，服务端零知识 | 冲突、增量同步、成员共享复杂 | 只做备份适配器 |
| 独立零知识同步服务 | 真正多端、版本化、可共享、可撤销 | 需要新增服务和账号体系 | 推荐正式方案 |

---

## 5. 密钥体系

### 5.1 密钥层级

```text
Master Password
  └─ Argon2id(password, user_salt, params) → Master Key
       └─ 解包 encrypted_vault_key → Vault Key
            ├─ 包装 Device Private Key
            ├─ 包装个人 Item Key
            └─ 包装个人恢复材料

Device Unlock Key（仅本机系统密钥库）
  └─ 包装当前设备可用的 Vault Key 副本

Collection Key（共享集合）
  ├─ 包装共享 Item Key
  └─ 分别使用每个成员的公钥封装
```

### 5.2 推荐算法

| 用途 | 推荐算法 |
|------|----------|
| 主密码派生 | Argon2id，参数可升级并存入 profile |
| 条目加密 | AES-256-GCM，复用当前依赖但强制随机唯一 nonce |
| 密钥封装 | AES-256-GCM + 独立 AAD，或标准化 HPKE 实现 |
| 成员密钥交换 | X25519 + HKDF-SHA256 |
| 设备/变更签名 | Ed25519 |
| 重复密码检测 | 仅在解锁后的本地内存中比较，默认不持久化密码摘要 |
| 内存清理 | `zeroize` + `secrecy` |

不继续使用当前 `SHA256(SQLite 中 seed)` 作为新密码库根密钥。当前 `secure_credential_secret_seed` 和 `credential_vault_secret_seed` 需要迁移到系统密钥库或由 Vault Key 重新包装。

### 5.3 主密码与恢复

- 服务端只保存认证 verifier 和 KDF 参数，不保存主密码。
- 修改主密码默认只重包 Vault Key，不重加密全部条目。
- 忘记主密码后默认不可恢复。
- v0.3 可提供一次性 Recovery Key、可信设备批准和组织账户恢复。
- Recovery Key 只显示一次，支持打印或加密文件保存。

### 5.4 锁定状态

状态机：

```text
uninitialized → locked → unlocked → background_locked → locked
                         └──────────→ reauth_required
```

- 应用启动默认锁定。
- 系统生物识别只用于释放 Device Unlock Key。
- 高风险操作要求重新输入主密码或完成平台生物识别。
- 自动锁定支持立即、1/5/15/30 分钟、系统锁屏和应用退出。
- 锁定时清除内存中的 Vault Key、Item Key、搜索索引和明文缓存。

---

## 6. 功能范围

### 6.1 v0.1：本地通用密码库与系统密钥库

#### 密码库初始化与解锁

- 创建主密码并显示强度。
- 生成 Vault Key、设备密钥和恢复密钥。
- 系统 Keychain / Credential Manager / Secret Service 绑定。
- 主密码解锁、系统生物识别解锁、手动锁定和自动锁定。
- 展示当前安全等级和降级原因。

说明：统一 `keyring` 适配器只保证应用秘密的安全存取，不保证所有平台都提供同一种生物识别提示。生物识别门控需按 macOS LocalAuthentication、Windows Hello 等平台能力单独实现；Linux 不支持时回退到主密码，不降级为无认证自动解锁。

#### 条目类型

- 登录：名称、用户名、密码、多个 URI、匹配规则、备注。
- 安全笔记。
- 基础身份信息。
- 银行卡仅在 v0.2 开放；v0.1 先预留类型。
- 自定义字段：文本、隐藏字段、布尔值。
- TOTP Secret 预留，v0.2 再生成验证码。

#### 基础能力

- 新建、编辑、软删除、恢复和永久删除。
- 文件夹、标签、收藏、最近使用。
- 密码生成器：长度、字符集、排除相似字符、Passphrase。
- 密码强度和重复密码提示。
- 显示密码、复制用户名、复制密码。
- 剪贴板 15/30/60 秒自动清除。
- 显示和复制操作写本地安全审计，但审计不包含明文。

#### 导入导出

- 导入 Chrome / Edge / Firefox / Bitwarden 常见 CSV/JSON。
- 导入前本地预览、字段映射、重复检测和逐项确认。
- 加密备份导出为默认路径。
- 明文 CSV/JSON 导出属于高风险操作，要求重新认证、明确确认和审计。

### 6.2 v0.2：浏览器扩展与个人多端同步

#### 浏览器扩展

- Chrome / Edge Manifest V3 首版。
- 当前站点候选账号提示。
- 用户触发自动填充，不默认页面加载即填充。
- 新登录/修改密码后的保存提示。
- 密码生成器和复制能力。
- 域名匹配：精确 host、base domain、自定义规则、永不填充列表。
- iframe、HTTP 页面、混合内容和可疑域名显示风险提示。

#### 浏览器桥接模式

首版采用 Native Messaging：

```text
Content Script
  → Extension Service Worker
  → chrome.runtime.connectNative()
  → tauri-ssh-browser-host sidecar
  → Unix Domain Socket / Named Pipe
  → Tauri SSH Rust Vault Service
```

约束：

- `allowed_origins` 固定正式扩展 ID，不使用通配符。
- Sidecar 不保存 Vault Key，不直接打开 SQLite。
- 桌面应用锁定时，扩展只能显示“密码库已锁定”。
- Native Messaging 消息使用 request ID、单次 nonce、短期 session 和响应签名。
- `stdout` 只写协议 JSON，日志只写 `stderr` 且必须脱敏。

#### 独立扩展模式

v0.2 后半阶段支持扩展保存加密缓存并直接连接同步服务：

- 使用 WebCrypto 解锁本地加密缓存。
- 扩展独立锁定，不共享桌面内存密钥。
- 浏览器存储中只有密文、KDF 参数和设备公钥。
- 禁止在 `chrome.storage.sync` 保存密钥或条目明文。

#### 个人同步

- 注册或绑定同步账号。
- 添加设备、二维码配对、一次性设备批准。
- 增量拉取和推送。
- 离线变更队列。
- 软删除 tombstone。
- 设备列表、最后同步时间、远程吊销设备。
- 新设备首次同步必须由主密码、Recovery Key 或已授权设备批准。

### 6.3 v0.3：共享与组织

- 创建组织、成员邀请和成员移除。
- Collection：名称、描述、成员、角色。
- 角色：Owner、Admin、Editor、Viewer。
- 条目从个人库移动或复制到集合。
- 共享条目使用 Collection Key，不直接复用个人 Vault Key。
- 成员加入时使用其公钥封装 Collection Key。
- 成员被移除后轮换 Collection Key；高敏集合可异步重包所有 Item Key。
- Viewer 默认可使用但不能导出；是否允许查看明文由集合策略控制。
- 分享操作、成员变更、导出、批量读取全部进入审计。
- v0.3 不实现“共享后阻止成员截图/手抄密码”这类不可保证能力。

### 6.4 明确暂不做

- 不直接读取或写入 Chrome / Edge / Firefox 内建密码数据库。
- 不读取浏览器 Cookie、Session 或登录态作为密码。
- 不允许 MCP、AI Skill、Runbook 批量读取个人密码库。
- 不做服务端解密、服务端搜索明文或服务端密码强度分析。
- 不做静默明文导出。
- 不承诺成员已读取密码后的远程“收回明文”。
- v0.1 不做 Passkey 托管，待浏览器和移动端协议稳定后单独规划。

---

## 7. 数据模型

密码库敏感元数据也应加密。SQLite 只保留同步、版本和索引所需的最小明文字段。

### 7.1 vault_profiles

```sql
CREATE TABLE vault_profiles (
    vault_id                    TEXT PRIMARY KEY,
    account_id                  TEXT DEFAULT NULL,
    kdf_type                    TEXT NOT NULL DEFAULT 'argon2id',
    kdf_salt                    TEXT NOT NULL,
    kdf_params_json             TEXT NOT NULL,
    encrypted_vault_key         TEXT NOT NULL,
    encrypted_vault_key_nonce   TEXT NOT NULL,
    auth_verifier               TEXT NOT NULL,
    state                       TEXT NOT NULL DEFAULT 'locked',
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
```

`auth_verifier` 必须与加密用 Master Key 域分离，避免同一派生值同时用于认证和加密。

### 7.2 vault_items

```sql
CREATE TABLE vault_items (
    item_id                     TEXT PRIMARY KEY,
    owner_type                  TEXT NOT NULL,
    owner_id                    TEXT NOT NULL,
    revision                    INTEGER NOT NULL DEFAULT 1,
    encrypted_payload           TEXT NOT NULL,
    payload_nonce               TEXT NOT NULL,
    wrapped_item_key            TEXT NOT NULL,
    wrapped_item_key_nonce      TEXT NOT NULL,
    aad_version                 INTEGER NOT NULL DEFAULT 1,
    deleted_at                  TEXT DEFAULT NULL,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
```

`encrypted_payload` 内包含名称、用户名、密码、URI、备注、自定义字段和 TOTP Secret。服务端不建立这些字段的明文索引。

### 7.3 vault_devices

```sql
CREATE TABLE vault_devices (
    device_id                   TEXT PRIMARY KEY,
    vault_id                    TEXT NOT NULL,
    display_name                TEXT NOT NULL,
    platform                    TEXT NOT NULL,
    encryption_public_key       TEXT NOT NULL,
    signing_public_key          TEXT NOT NULL,
    encrypted_private_keys      TEXT NOT NULL,
    trust_state                 TEXT NOT NULL,
    last_sync_cursor            TEXT NOT NULL DEFAULT '',
    last_seen_at                TEXT DEFAULT NULL,
    revoked_at                  TEXT DEFAULT NULL,
    created_at                  TEXT NOT NULL
);
```

### 7.4 vault_sync_changes

```sql
CREATE TABLE vault_sync_changes (
    change_id                   TEXT PRIMARY KEY,
    vault_id                    TEXT NOT NULL,
    device_id                   TEXT NOT NULL,
    entity_type                 TEXT NOT NULL,
    entity_id                   TEXT NOT NULL,
    base_revision               INTEGER NOT NULL,
    new_revision                INTEGER NOT NULL,
    encrypted_change            TEXT NOT NULL,
    change_nonce                TEXT NOT NULL,
    signature                   TEXT NOT NULL,
    sync_state                  TEXT NOT NULL DEFAULT 'pending',
    created_at                  TEXT NOT NULL
);
```

### 7.5 vault_collections 与成员密钥

```sql
CREATE TABLE vault_collections (
    collection_id              TEXT PRIMARY KEY,
    organization_id            TEXT NOT NULL,
    encrypted_metadata         TEXT NOT NULL,
    metadata_nonce             TEXT NOT NULL,
    key_version                INTEGER NOT NULL DEFAULT 1,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);

CREATE TABLE vault_collection_member_keys (
    collection_id              TEXT NOT NULL,
    member_id                  TEXT NOT NULL,
    key_version                INTEGER NOT NULL,
    wrapped_collection_key     TEXT NOT NULL,
    wrap_nonce                 TEXT NOT NULL,
    member_role                TEXT NOT NULL,
    revoked_at                 TEXT DEFAULT NULL,
    PRIMARY KEY (collection_id, member_id, key_version)
);
```

---

## 8. 冲突与同步协议

### 8.1 同步对象

- item upsert / delete。
- folder / tag。
- device trust / revoke。
- organization / collection / membership。
- policy。

### 8.2 冲突策略

- 每个实体有单调递增 revision。
- 客户端推送必须携带 `baseRevision`。
- 服务端当前 revision 不一致时返回 conflict，不做静默 last-write-wins。
- 不同字段可在客户端进行三方合并。
- 密码、TOTP Secret、附件等敏感字段发生并发修改时保留两个版本，由用户选择。
- 删除使用 tombstone，并保留至少 30 天以同步到离线设备。

### 8.3 同步安全

- 每个 change 由设备 Ed25519 私钥签名。
- 服务端只接受 active device 的签名。
- 吊销设备后拒绝其新变更，但保留历史审计。
- 防重放：`changeId + deviceId + revision + nonce` 唯一。
- TLS 是传输层保护，不能替代客户端端到端加密。

---

## 9. Rust 模块规划

```text
src-tauri/src/
├── commands/
│   ├── password_vault.rs
│   ├── vault_sync.rs
│   └── vault_sharing.rs
├── services/
│   ├── password_vault.rs
│   ├── vault_crypto.rs
│   ├── system_keychain.rs
│   ├── vault_sync.rs
│   ├── vault_sharing.rs
│   ├── vault_import_export.rs
│   └── browser_bridge.rs
├── database/
│   └── vault.rs
└── models/
    └── vault.rs

browser-extension/
├── manifest.json
├── src/background/
├── src/content/
├── src/popup/
└── src/crypto/

browser-host/
└── Rust Native Messaging sidecar

sync-server/
└── 独立可自托管的 Rust/Axum 零知识同步服务
```

### 9.1 SystemSecretStore 接口

```rust
trait SystemSecretStore {
    fn availability(&self) -> Result<KeychainAvailability, AppError>;
    fn get(&self, service: &str, account: &str) -> Result<Option<SecretVec<u8>>, AppError>;
    fn set(&self, service: &str, account: &str, value: SecretVec<u8>) -> Result<(), AppError>;
    fn delete(&self, service: &str, account: &str) -> Result<(), AppError>;
}
```

推荐使用 `keyring` crate 统一接入系统安全存储，并为 macOS、Windows、Linux 分别做真实环境测试。系统 API 调用只发生在 Rust 侧，不向 React 暴露通用 keychain 读写 Command。

### 9.2 核心 Commands

```text
initialize_password_vault
get_password_vault_status
unlock_password_vault
unlock_password_vault_with_device
lock_password_vault
list_password_vault_items
get_password_vault_item
upsert_password_vault_item
delete_password_vault_item
restore_password_vault_item
reveal_password_vault_secret
copy_password_vault_field
generate_password
import_password_vault
export_password_vault_encrypted
export_password_vault_plaintext
list_vault_devices
approve_vault_device
revoke_vault_device
sync_password_vault
list_vault_collections
share_vault_item
revoke_vault_share
```

高风险 Commands 必须接收解锁会话 ID，并在 Service 层校验重新认证时间。Dev HTTP API 不提供 reveal、copy、plaintext export 和 keychain 操作。

---

## 10. 前端与交互规划

### 10.1 菜单

```text
安全凭证
├── 概览
├── 通用密码库
├── 共享与组织
├── 设备与同步
├── 浏览器扩展
├── 凭证库
├── Git 工作区
├── 代码审核
├── 会话
├── MCP 接入
├── 审计
└── 策略
```

### 10.2 通用密码库页面

- 左侧：全部、收藏、登录、安全笔记、身份、银行卡、文件夹、回收站。
- 中间：条目列表、搜索、排序、同步状态。
- 右侧 Drawer：查看和编辑。
- 密码默认掩码；显示、复制分别触发 Command。
- URI 只允许 `http`、`https` 和显式支持的应用协议。
- 复制后显示倒计时和清除状态。

### 10.3 解锁页面

- 主密码输入框。
- 系统生物识别/设备解锁按钮。
- 当前设备、离线状态和同步账号提示。
- 锁定原因：启动锁定、超时、系统锁屏、设备吊销、KDF 升级。
- 不在浏览器 LocalStorage、Zustand persist 或 Tauri Store 保存主密码和 Vault Key。

### 10.4 设备与同步页面

- 当前设备和其它设备列表。
- 信任状态、平台、最近同步、版本。
- 添加设备二维码。
- 远程吊销。
- 手动同步和冲突中心。
- 同步服务地址和证书状态。

---

## 11. 权限、审计与安全策略

### 11.1 Capabilities

- Keychain 操作封装在自有 Rust Command 内，不给前端通用系统密钥库权限。
- 文件导入/导出使用 Dialog，并限制路径交互。
- 剪贴板仅开放具体 `copy_password_vault_field` Command，不开放任意后台读取剪贴板。
- 浏览器 Bridge 使用本地 Socket/Named Pipe，不开放公网监听。
- 同步网络请求由 Rust Sync Service 发起，域名由用户配置和策略校验。

### 11.2 审计事件

```text
vault_initialize
vault_unlock_success / vault_unlock_failed
vault_lock
item_create / item_update / item_delete / item_restore
secret_reveal / secret_copy / browser_fill
vault_import / vault_export_encrypted / vault_export_plaintext
device_pair / device_approve / device_revoke
sync_push / sync_pull / sync_conflict
organization_create / member_invite / member_revoke
item_share / item_unshare / collection_key_rotate
```

审计不记录：

- 主密码。
- 密码或密码片段。
- TOTP Secret 和验证码。
- 完整银行卡号、安全码。
- Vault Key、Item Key、Collection Key。
- Native Messaging 完整 payload。

### 11.3 重新认证矩阵

| 操作 | 已解锁即可 | 需要重新认证 |
|------|------------|--------------|
| 列表、搜索、普通填充 | 是 | 否 |
| 显示单个密码 | 可配置 | 默认是 |
| 复制密码 | 可配置 | 高敏条目需要 |
| 明文导出 | 否 | 是 |
| 添加新设备 | 否 | 是 |
| 生成 Recovery Key | 否 | 是 |
| 创建共享、提升成员角色 | 否 | 是 |
| 永久删除 | 否 | 是 |

---

## 12. 迁移策略

### 12.1 现有安全凭证密钥迁移

当前安全凭证使用 AES-GCM，但根 seed 存在 SQLite 配置中。迁移步骤：

1. 初始化新 Vault Key。
2. 用户解锁后读取旧 seed 并解密现有凭证。
3. 使用新的密钥层级重加密或包装。
4. 将设备解锁材料写入系统密钥库。
5. 完成校验后删除 SQLite 中的旧 seed。
6. 保留迁移版本、失败回滚点和审计记录，不记录任何明文。

迁移失败时保持旧数据可用，不允许半迁移状态覆盖原密文。

### 12.2 旧 credential_vault

- 旧 `credential_vault` 继续作为兼容入口。
- 用户可选择迁移为安全凭证或通用密码条目。
- SSH、Token、API Key 默认迁入安全凭证。
- 网站账号和普通应用登录默认迁入通用密码库。
- 迁移后旧记录先禁用，不立即物理删除。

---

## 13. 实施阶段

### 阶段 0：威胁模型与密码学基线

- 确认资产、攻击者、信任边界和恢复模型。
- 固化加密格式、AAD、KDF 参数和测试向量。
- 评审系统密钥库和降级策略。
- 输出数据库迁移设计。

验收：密码学格式可被桌面、移动和浏览器共同实现；安全评审无阻断项。

### 阶段 1：本地密码库内核

- 新增表、模型、Database、Service、Commands。
- 主密码初始化、解锁、锁定。
- Item Key 和 Vault Key 分层。
- CRUD、密码生成、搜索和剪贴板清理。

验收：离线可完整使用；SQLite 和日志中搜索不到测试密码明文。

### 阶段 2：系统密钥库

- 实现 `SystemSecretStore`。
- macOS、Windows、Linux 可用性检测。
- 设备解锁和自动锁定。
- 迁移旧 seed。

验收：删除系统密钥库条目后必须回到主密码解锁；不能从 SQLite 单独解密密码库。

### 阶段 3：导入导出

- Chrome/Edge/Firefox/Bitwarden 格式映射。
- 重复检测和导入预览。
- 加密备份与恢复。
- 高风险明文导出。

验收：失败导入不产生半条目；临时文件被清理。

### 阶段 4：浏览器 Bridge

- Manifest V3 扩展。
- Native Messaging Sidecar 和安装清单。
- 候选匹配、填充、保存提示和生成密码。
- 锁定状态联动。

验收：非允许扩展 ID 无法连接；桌面锁定后无法填充；HTTP/可疑域名有风险提示。

### 阶段 5：个人零知识同步

- 独立同步服务。
- 账号、设备、密文 revision、签名和 tombstone。
- 桌面、移动和扩展同步客户端。
- 配对、冲突和吊销。

验收：服务端数据库无法恢复明文；离线双端修改可产生可处理冲突；吊销设备不能继续推送。

### 阶段 6：共享与组织

- 用户公钥、组织、集合、成员角色。
- Collection Key 包装和轮换。
- 分享、移除、审计和策略。

验收：新成员只能解密获授权集合；成员移除后不能解密后续版本；服务端仍保持零知识。

### 阶段 7：安全审计与发布

- 密码学专项审查。
- 浏览器扩展权限审查。
- 多平台 Keychain 测试。
- 同步协议重放、篡改和并发测试。
- 依赖供应链、升级和回滚测试。

---

## 14. 验收标准

### 14.1 功能验收

- 可创建、编辑、删除、恢复和搜索登录条目。
- 可生成、显示、复制和自动清除密码。
- 可使用主密码和系统设备解锁。
- 浏览器扩展可完成候选提示、填充和保存。
- 至少两个桌面设备和一个浏览器扩展可同步。
- 可吊销设备并阻止后续同步。
- 可将条目共享给集合成员并撤销。

### 14.2 安全验收

- SQLite、日志、审计、崩溃信息和同步服务中无密码明文。
- 主密码不落盘、不进入日志、不通过 Dev API。
- 仅凭同步服务数据库不能解密条目。
- 仅凭本地 SQLite 且没有主密码/系统密钥库不能解密条目。
- MCP tools/list 中不存在个人密码读取工具。
- Native Messaging 只允许固定扩展 ID。
- 明文导出、设备添加、Recovery Key、成员提权必须重新认证。
- 所有 AEAD 解密都验证 AAD、版本、owner 和 item ID。

### 14.3 多端验收

- Windows、macOS、Linux 系统密钥库分别验证。
- Chrome、Edge 扩展验证；Firefox 在 v0.2 后续兼容。
- 移动端本地密码库不依赖桌面端在线。
- 网络断开、恢复、重复提交、乱序提交和冲突均有确定结果。

### 14.4 UI 验收

- 主密码、密码字段和安全笔记默认掩码。
- 锁定状态不能通过前端路由绕过。
- 页面刷新后不从 Web Storage 恢复明文。
- 暗色/亮色主题、缩放、长域名和长用户名无溢出。
- 使用真实 Tauri 页面和浏览器扩展验证，不只验证构建结果。

---

## 15. 关键风险与规避

### 15.1 把系统密钥库误当完整密码库

风险：跨平台行为不一致，难以版本化、搜索和共享。

规避：系统密钥库只保存设备解锁材料，业务条目继续使用加密 SQLite 和零知识同步。

### 15.2 浏览器原生密码库无公开接口

风险：承诺直接读写 Chrome/Edge 密码后无法实现，或被迫操作不稳定的内部数据库。

规避：自有扩展负责自动填充；原生库迁移采用主动导入/导出。

### 15.3 现有 SQLite seed 降低安全等级

风险：复制数据库即可同时获得密文和根 seed。

规避：新 Vault Key 不落明文；旧 seed 分阶段迁移到新密钥层级并从 SQLite 删除。

### 15.4 共享撤销的能力边界

风险：成员在撤销前已经看到或复制明文，系统无法追回。

规避：产品文案明确边界；撤销只阻止后续访问和新版本解密；敏感集合支持成员水印、审计和定期轮换，但不声称能收回已知秘密。

### 15.5 自研密码学协议风险

风险：算法组合、AAD、nonce、密钥轮换或兼容性错误导致泄露或数据丢失。

规避：优先成熟 crate 和标准协议；冻结格式并维护跨客户端测试向量；上线前进行独立安全审查。

---

## 16. 推荐首个可提交单元

首个开发单元只完成密码库密码学基线和本地锁定状态，不同时开发浏览器与同步：

1. 新增 `vault_crypto` 模块和固定测试向量。
2. 使用 Argon2id 派生 Master Key。
3. 生成并包装 Vault Key。
4. 新增 `vault_profiles` 和最小 `vault_items` 表。
5. 实现 initialize / unlock / lock / status Commands。
6. 增加锁定页，不开放条目 CRUD。
7. 验证应用重启、错误主密码、篡改密文和内存清理。

完成这一单元后，再进入条目 CRUD 和系统密钥库接入。这样可以先稳定最难迁移的密钥格式，避免 UI、同步和浏览器扩展建立在不稳定的加密协议上。

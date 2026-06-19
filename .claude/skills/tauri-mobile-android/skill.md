---
name: tauri-mobile-android
description: Tauri Mobile Android 打包专项。NDK 中文路径修复、minSdkVersion、versionCode 严格递增、APK/AAB 产物处理、侧载分发的 Tauri Updater 限制、签名 keystore。
effort: high
---

# Tauri Mobile Android Packaging

## 触发场景

- 给桌面 Tauri 应用增加 Android 配套（mobile-tauri/ 子项目）
- Android 打包失败（NDK / 链接器 / 路径问题）
- versionCode / versionName 规则疑问
- APK 侧载分发的更新策略

## 触发词

Android、APK、AAB、NDK、ld.lld、versionCode、tauri android、minSdkVersion、侧载

---

## 前置：mobile-tauri/ 子项目骨架

见 `mobile-app-architecture` skill。本 skill 假定：
- 项目根有 `mobile-tauri/` 子目录
- `vite.mobile.config.ts` + `dist-mobile/` 已就绪
- mobile-tauri/src-tauri/tauri.conf.json 的 `frontendDist` 指向 `../../dist-mobile`

---

## 关键坑 #1：NDK 中文路径

### 现象

```
ld.lld: error: unable to find library -l<...>
linker error: cannot find /<中文路径>/target/...
```

### 根因

Android NDK ld.lld（Windows 版）**不支持中文路径**。如果项目位于 `E:/my/桌面软件tauri/...` 这类含中文的目录，cargo 中间产物（.o / .rlib / .so）路径里的中文字符会让 ld 找不到文件。

### 修复

在 `mobile-tauri/.cargo/config.toml` 强制 target-dir 到 ASCII 目录：

```toml
[build]
target-dir = "C:/cargo-target/<your-project>-mobile"
```

**注意**：
- 路径必须**全部 ASCII**（盘符 + 全英文目录名）
- 与主项目 target-dir 分开（避免 Linux 桌面 target 与 Android target 串）
- 首次构建前确保该目录可写（Tauri CLI 不会自动创建父目录，必要时 `mkdir -p`）

### 副作用

- IDE（RustRover / VSCode rust-analyzer）可能仍按默认 target-dir 找中间产物，需要在 IDE 配置里同步指向新路径
- `cargo clean` 不清这个目录，需手动 `rm -rf C:/cargo-target/<...>`

---

## 关键配置

### tauri.conf.json (mobile-tauri/src-tauri/)

```json
{
  "bundle": {
    "android": {
      "minSdkVersion": 26
    }
  }
}
```

| 字段 | 推荐值 | 说明 |
|------|--------|------|
| `minSdkVersion` | 26（Android 8.0） | Tauri 2 支持的最低；低于 26 webview 能力受限 |
| `targetSdkVersion` | Tauri CLI 默认 | 通常无需手动指定 |

### Cargo.toml release profile（与桌面端一致）

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

减少 APK 体积。

---

## 版本号规则（versionCode / versionName）

文件：`mobile-tauri/src-tauri/gen/android/app/build.gradle.kts`（首次 `tauri android init` 后生成）

```kotlin
versionName = "0.3.6"
versionCode = 306   // 必须严格递增！
```

### versionCode 规则

- **必须严格大于**已安装版本，否则 Android **拒绝覆盖安装**（用户看到"应用未安装"）
- 推荐策略：`versionCode = major*10000 + minor*100 + patch`
  - 0.1.0 → 100
  - 0.3.6 → 306
  - 1.0.0 → 10000

### 同步 4 处版本号

| 文件 | 字段 |
|------|------|
| `mobile-tauri/src-tauri/tauri.conf.json` | `version` |
| `mobile-tauri/src-tauri/Cargo.toml` | `version` |
| `mobile-tauri/package.json` | `version` |
| `mobile-tauri/src-tauri/gen/android/app/build.gradle.kts` | `versionName` + `versionCode` |

🔴 关键：移动端版本号与桌面端**独立**。桌面 v3.5.0 时移动端可能还在 v0.2.1。

---

## 构建命令

```bash
# 初始化（首次）
cd mobile-tauri
pnpm install
pnpm android:init   # 生成 gen/android/

# 开发（连真机或模拟器）
pnpm android:dev

# 生产构建（先把 PC 仓库的 dist-mobile 准备好，再打 APK）
pnpm android:build  # 内部：先 pnpm --dir .. mobile:build，再 tauri android build
```

---

## 产物路径

```
mobile-tauri/src-tauri/gen/android/app/build/outputs/
├── apk/universal/release/app-universal-release.apk        # 通用 APK
└── bundle/universalRelease/app-universal-release.aab      # AAB（应用市场）
```

**侧载分发**首选 APK；上 Google Play / 国内应用市场用 AAB。

---

## 关键限制：侧载分发不支持 Tauri Updater 静默更新

| 分发方式 | 自动更新 |
|---------|---------|
| Google Play | ✅ 商店自动 |
| 国内应用市场 | ✅ 商店自动 |
| **APK 侧载** | ❌ Tauri updater 不可用，必须用户手动下载安装 |

因此移动端发布**不生成 update.json**，下载页直链 APK 即可。详见 `release-publish` skill 的"双线发布"章节。

---

## 签名

```
mobile-tauri/
├── keystore.properties.example   # commit 进仓库（占位）
└── keystore.properties           # ❌ 不要 commit（含路径/密码）
```

`keystore.properties` 内容示例：

```properties
storeFile=/abs/path/to/release.keystore
storePassword=<your password>
keyAlias=<your alias>
keyPassword=<your key password>
```

CI 中通过 secrets 注入 keystore 二进制 + 上述四个字段。

---

## 必备的 .gitignore 条目

```
# Android 中间产物
mobile-tauri/src-tauri/gen/android/app/build/
mobile-tauri/src-tauri/gen/android/.gradle/
mobile-tauri/src-tauri/gen/android/local.properties

# 签名（永远不 commit）
mobile-tauri/keystore.properties
mobile-tauri/*.keystore
mobile-tauri/*.jks

# Cargo target（如果配 target-dir 到项目内则忽略它；推荐配到 C:/cargo-target/）
mobile-tauri/src-tauri/target/
```

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 项目位于中文路径，直接 `tauri android build` | 配 `.cargo/config.toml` target-dir 到 ASCII 目录 |
| versionCode 复用、写小、忘改 | 严格递增；推荐用 `major*10000+minor*100+patch` 公式 |
| `tauri android init` 后改了 build.gradle.kts 又被覆盖 | 改 gen/ 内容只在打包前临时写；版本号同步用脚本 |
| 用 PWA 走 Tauri Updater 自动更新 | 侧载 APK 不支持，改提示用户手动下载 |
| keystore.properties commit 进仓库 | 只 commit `.example`；实文件 .gitignore |
| `min-sdk-version` 低于 24 | Tauri 2 webview 能力受限；推荐 26 |

---

## 相关 skill

- `mobile-app-architecture` — 整体架构与 mobile-tauri/ 骨架
- `tauri-packaging` — 桌面端打包基础
- `release-publish` — "双线发布"章节
- `bug-detective` — NDK 中文路径错误表

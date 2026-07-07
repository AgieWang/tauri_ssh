---
name: bug-detective
description: |
  排查已发生的问题、定位 Bug 原因。

  触发场景：
  - 代码运行报错，需要定位原因
  - 功能不正常，需要排查
  - Tauri Command 返回错误，需要分析
  - 日志分析、调试代码

  触发词：Bug、报错、不工作、调试、排查、为什么、出问题、失败、不生效、无效、找不到原因、定位问题
---

# Bug 排查指南

## 排查方法论

### 1. 复现问题
- 确认问题的具体表现
- 收集错误信息（终端日志、浏览器控制台、Rust panic 信息）
- 确认问题的触发条件
- 确认问题出现在哪个平台（Windows/macOS/Linux）

### 2. 缩小范围
- 前端 (React) or 后端 (Rust)？
- IPC 通信层的问题？
- 权限 (Capabilities) 不足？
- 哪个 Command/组件？
- 什么时候开始出现？

### 3. 定位根因
- 阅读相关 Rust/TypeScript 代码
- 检查终端日志（Rust println!/log）
- 检查浏览器 DevTools 控制台
- 添加 `dbg!()` 宏（Rust）或 `console.log`（TS）
- 对比正常 vs 异常的数据

### 4. 验证修复
- 修复后验证问题已解决
- 在所有目标平台上测试
- 确认没有引入新问题

---

## 常见问题分类

### Rust 后端常见问题

| 症状 | 可能原因 | 排查方法 |
|------|---------|---------|
| Command 调用无响应 | 函数名未在 `generate_handler!` 注册 | 检查 `lib.rs` 的 handler 列表 |
| `invoke` 返回错误 | Rust 侧 panic 或返回 Err | 检查终端 Rust 错误输出 |
| 类型序列化失败 | struct 缺少 Serialize/Deserialize derive | 添加 `#[derive(Serialize, Deserialize)]` |
| State 获取失败 | 未在 Builder 中 `.manage()` 注册 | 检查 Builder 链式调用 |
| 编译错误 | 所有权/借用/生命周期问题 | 阅读 Rust 编译器错误提示 |
| 插件功能不可用 | Capabilities 未声明权限 | 检查 `capabilities/default.json` |
| 批量/高频写入时文件名互相覆盖（用 timestamp+纳秒拼名） | Windows 系统时钟分辨率在极短间隔内可能给出相同值，多次调用拼出相同文件名 | 加进程内 `AtomicU64` 计数器拼到文件名末尾：`{ts}_{nanos}_{seq:06}.{ext}` 作为防冲突兜底 |

### React 前端常见问题

| 症状 | 可能原因 | 排查方法 |
|------|---------|---------|
| 页面空白 | JS 错误 | 打开 DevTools 控制台 (F12) |
| invoke 调用报错 | Command 名称拼写错误 | 确认 snake_case 函数名 |
| 状态不更新 | useState 闭包陷阱 | 使用函数式更新 `setState(prev => ...)` |
| 事件监听不生效 | 未清理旧监听器 | 在 useEffect 中返回 unlisten |
| 样式不生效 | CSS 冲突或选择器错误 | 使用 DevTools Elements 面板 |
| 页内拖拽光标显示 🚫、onDrop 不触发（antd Tree/react-dnd 等） | Tauri 窗口 `dragDropEnabled` 默认 true，WebView 吞掉 HTML5 dragover/drop | `tauri.conf.json` 窗口配置加 `"dragDropEnabled": false`，重启 dev |
| 右键菜单 Dropdown（`trigger={['contextMenu']}`）包裹节点后 antd Tree 拖不动 | rc-trigger ref 转发 + mousedown 拦截破坏原生 drag 绑定 | 改用 Tree 级 `onRightClick` + 全局定位 Dropdown（幻影锚点） |
| AntD Modal 编辑/克隆时表单全空（新建正常） | `destroyOnClose` + 在 `open=false` 时 `setFieldsValue`；此时 Form.Item 尚未挂载到 form 实例，赋值丢失 | 用 `key={formKey}` + `initialValues={pendingValues}` 让 Form 每次打开重挂载吃 initialValues，不依赖 setFieldsValue 时序 |
| VitePress 首页自由 md 内容被夹在中间、外部 CSS 怎么写都改不动 | `index.md` 手写了 `<div class="vp-doc" style="max-width: 960px; ...">` 包裹；inline style 优先级最高，外部选择器 + !important 都压不过 | 直接删掉 `index.md` 里手写的 wrapper，让 VitePress 默认 `.vp-doc.container` 自动处理（和 Hero/Features 宽度对齐） |
| `@tanstack/react-virtual` 列表一条也不渲染 | 滚动容器只设 `maxHeight` 没给 `height`，又叠了 `contain: strict`（含 `contain: size`），浏览器把容器计算成 0 高度 → virtualizer 算不出可见行 | 去掉 `contain: strict` 或换成 `contain: content`（= layout paint style，不含 size），也可以直接给明确的 `height` |
| AntD `<Sider>` 的 `style={{display:'flex'}}` 无效、子元素还是纵向堆叠 | AntD Sider 内部把 children 包了一层 `.ant-layout-sider-children` 默认 block 布局，Sider 上的 flex 作用在 aside 外层，不传递到 children | 在 Sider 内包一层自己的 flex `<div style={{display:'flex',height:'100%'}}>` 再放 children |

### IPC 通信常见问题

| 症状 | 可能原因 | 排查方法 |
|------|---------|---------|
| invoke 超时 | Rust 侧阻塞主线程 | 改用 async Command |
| 参数传递失败 | 参数类型不匹配 (camelCase vs snake_case) | 检查前后端参数名映射 |
| 返回值为空 | Rust 函数签名返回 `()` | 确认返回 `Result<T, String>` |

### 开发环境 / 本地脚本常见问题

| 症状 | 可能原因 | 排查方法 |
|------|---------|---------|
| `curl http://localhost:xxxx/` 返回 502 但服务明明在跑 | 本机设置了 http_proxy/https_proxy，curl 把 localhost 请求也走代理去外网 | 加 `--noproxy '*'`（或 `NO_PROXY=localhost,127.0.0.1`）；Node fetch 同理，用 `{ proxy: false }` |
| Node.js 脚本 `fs.readFileSync('/tmp/xx.json')` 在 Windows 报 `ENOENT E:\tmp\xx.json` | Node 在 Windows 下把 Unix 路径 `/tmp` 解析成当前盘根 `E:\tmp`（不存在） | 用 `os.tmpdir()` 或放在项目内的相对路径，别硬写 `/tmp` |
| `pnpm dev` 输出了 `Port 5173 in use, trying 5174`，后续自动化脚本 curl 5173 永远 404 | 上一次 dev 进程未退，Vite 自动换端口 | 读 dev 日志确认实际端口；或 `npx kill-port 5173` 后重启 |
| Windows 下 `bash -c "set VAR=val && cmd"` 或 `$env:VAR='val'; cmd` 没生效 | Codex 的 Bash 跑在 Git Bash (MSYS2)，用 bash 语法 `export VAR=val && cmd`，不是 CMD/PowerShell | 统一 `export VAR=val && cmd`，或在子进程里用 env: `{}` 传 |
| Android APK 每个新版本都被系统拦截「与已安装应用签名不同」，必须先卸载旧版才能升级 | CI workflow 没配 `ANDROID_KEYSTORE_BASE64` secret，gradle 用 runner 临时生成的 `debug.keystore` 签名，每次 build 签名都不同 | 本地一次性生成稳定 release keystore + 4 个 secret 注入 CI；workflow 加 `if: env.HAS_KEYSTORE == 'true'` 防 step 静默 skip；用 `apksigner verify --print-certs` 比对 SHA-256 指纹后再发布。详见 `release-publish` skill 移动端章节 |
| 移动端「检查更新」永远报「已是最新版本」，但下载页确实有新版 | `parseSemver` 没剥 `mobile-` 前缀，`mobile-vX.Y.Z` 经 `replace(/^v/, "")` 不变（以 m 开头）→ 正则不匹配 → 返回 null → `compareSemver` 视为同版本 | 解析前先 `s.replace(/^mobile-/, "").replace(/^v/, "")`；老用户必须从下载页手动拉一次新版才能恢复 |

---

## 调试工具

### Rust 调试
```rust
// println! 输出到终端
println!("Debug: {:?}", variable);

// dbg! 宏（输出文件名/行号/值）
dbg!(&my_variable);

// 使用 log crate
log::info!("Processing: {}", data);
log::error!("Failed: {}", err);
```

### TypeScript 调试
```typescript
// 浏览器控制台
console.log("invoke result:", result);
console.error("invoke failed:", error);

// 检查 invoke 调用
try {
  const result = await invoke("my_command", { arg1 });
  console.log("Success:", result);
} catch (e) {
  console.error("Failed:", e);
}
```

### DevTools 开启
```
// 开发模式自动开启 DevTools
// 生产模式可通过配置开启:
// tauri.conf.json → app.windows[0].devtools = true
```

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 不看 Rust 编译器错误提示 | Rust 编译器提示非常详细，先仔细阅读 |
| 不区分前端/后端/IPC 问题 | 先确定问题在哪个层，再深入排查 |
| 不检查 Capabilities | 插件功能不可用时首先检查权限声明 |
| 只在一个平台测试 | 跨平台问题需在所有目标平台验证 |
| 在中文路径下编译 Tauri Mobile (Android) | `mobile-tauri/.cargo/config.toml` 设 `target-dir = "C:/cargo-target/<project>"` 强制移到 ASCII 目录（NDK ld.lld 不识别中文） |
| Mobile 子项目 vite build 报 `Rollup failed to resolve "@/..."` | re-export 链中**不要用 `@/` 别名**，改成相对路径；vite/rollup 在 CI 对 re-export 链的别名解析特别敏感 |
| 桌面应用内嵌 frpc/easytier 等隧道二进制被杀软误报为木马 | 改为**引导用户自配反向代理**（应用只绑定本地端口） |

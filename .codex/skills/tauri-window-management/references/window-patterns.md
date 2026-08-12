# Tauri 窗口、标题栏与托盘模式参考

## 目录

1. `dragDropEnabled` 条件选择
2. 开发与正式托盘区分
3. 窗口配置与创建
4. 无边框标题栏与控制 API
5. 托盘和常见错误

## `dragDropEnabled` 的条件选择

Tauri 原生文件拖入与 WebView 页内 HTML5 拖拽可能争用 `dragover/drop`。当 Ant Design Tree、react-dnd、看板排序或分栏拖动在真实 Tauri WebView 中被拦截时，可将对应窗口的 `dragDropEnabled` 设为 `false`；修改后必须完整重启 `pnpm tauri dev`，仅前端 HMR 不会重载 Tauri 配置。

关闭后的代价是不能直接从操作系统文件管理器把文件拖入应用。产品需要 OS 文件拖入时，应保留原生能力并使用 Tauri drag-drop 事件，或提供文件对话框等替代交互；不能把 `false` 作为所有窗口默认值。修改前先列清页内拖拽与 OS 文件拖入两类需求，并分别做真实运行时测试。

## 开发与正式托盘区分

开发版和正式安装版可能同时驻留托盘。只有项目确有辨识需求且已有托盘实现时，才在 `cfg!(debug_assertions)` 分支为开发图标增加角标/`[DEV]` tooltip；正式版必须使用原图，不能把调试标识带入发布产物。优先复用项目现有 `tray.rs`/角标函数，不为普通窗口功能额外生成图标资源。

## 窗口配置

### tauri.conf.json 窗口配置

```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "我的应用",
        "width": 1024,
        "height": 768,
        "minWidth": 800,
        "minHeight": 600,
        "resizable": true,
        "center": true,
        "decorations": true,
        "transparent": false,
        "fullscreen": false,
        "alwaysOnTop": false
      }
    ]
  }
}
```

---

## 多窗口

### Rust 创建新窗口

```rust
use tauri::Manager;
use tauri::WebviewWindowBuilder;
use tauri::WebviewUrl;

#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    let _window = WebviewWindowBuilder::new(
        &app,
        "settings",
        WebviewUrl::App("index.html".into()),
    )
    .title("设置")
    .inner_size(600.0, 400.0)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
```

### TypeScript 创建新窗口

```typescript
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

const settingsWindow = new WebviewWindow("settings", {
  url: "/settings",
  title: "设置",
  width: 600,
  height: 400,
  center: true,
});

settingsWindow.once("tauri://created", () => {
  console.log("设置窗口已创建");
});
```

---

## 无边框窗口 + 自定义标题栏

### 配置

```json
{
  "app": {
    "windows": [{
      "decorations": false,
      "transparent": true
    }]
  }
}
```

### 自定义标题栏组件

```tsx
function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      style={{
        height: 30,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        background: "#1a1a2e",
        color: "white",
        padding: "0 8px",
        userSelect: "none",
      }}
    >
      <span>我的应用</span>
      <div>
        <button onClick={() => appWindow.minimize()}>—</button>
        <button onClick={() => appWindow.toggleMaximize()}>□</button>
        <button onClick={() => appWindow.close()}>✕</button>
      </div>
    </div>
  );
}
```

> `data-tauri-drag-region` 使该区域可拖拽移动窗口。

---

## 窗口控制 API

```typescript
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

await appWindow.minimize();          // 最小化
await appWindow.maximize();          // 最大化
await appWindow.unmaximize();        // 还原
await appWindow.toggleMaximize();    // 切换最大化
await appWindow.close();             // 关闭
await appWindow.hide();              // 隐藏
await appWindow.show();              // 显示
await appWindow.setTitle("新标题");   // 设置标题
await appWindow.setSize(new LogicalSize(800, 600));  // 设置大小
await appWindow.center();            // 居中
await appWindow.setAlwaysOnTop(true); // 置顶
```

---

## 系统托盘

```rust
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};
use tauri::menu::{Menu, MenuItem};

pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "quit" => app.exit(0),
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}
```

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 多窗口用同一个 label | 每个窗口 label 必须唯一 |
| 无边框窗口不加拖拽区域 | 添加 `data-tauri-drag-region` |
| 关闭窗口不清理资源 | 监听 close-requested 事件清理 |
| 不处理窗口创建失败 | 窗口可能已存在，需 catch 错误 |

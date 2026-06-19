# Mobile Icons

请把 1024×1024 的 PNG 源图放到项目根，然后在 mobile-tauri/ 目录运行：

```bash
pnpm tauri icon ../path/to/icon-1024.png
```

会自动生成本目录所需的多平台图标：

- `32x32.png` / `128x128.png` / `128x128@2x.png` — 通用
- `icon.ico` — Windows
- `icon.icns` — macOS
- `android/` — Android 各分辨率（首次 `pnpm android:init` 后生成）

如果 `tauri.conf.json` 中 `bundle.icon` 引用的图标不存在，构建会失败 —— 请先生成图标再打包。

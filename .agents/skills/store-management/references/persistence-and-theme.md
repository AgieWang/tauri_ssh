# 轻量偏好持久化与主题恢复

仅在 Zustand 状态需要通过 `tauri-plugin-store` 跨启动保存，或处理主题/语言偏好时读取。

## 适用数据

适合：主题、语言、侧栏布局、关闭行为等少量用户偏好。

不适合：

- 业务记录、历史列表或需要查询的数据：使用 SQLite。
- 密码、Token、私钥：使用项目安全凭据设施。
- 大文件或二进制：使用受控文件存储。

## 插件前置

确认三处都存在且版本与项目一致：

1. Rust 依赖及 `tauri_plugin_store::Builder` 注册。
2. 前端 `@tauri-apps/plugin-store` 依赖。
3. `capabilities/*.json` 中最小 store 权限。

## 加载和保存

```typescript
import { Store } from "@tauri-apps/plugin-store";

const store = await Store.load("settings.json");
const savedTheme = await store.get<unknown>("theme");
if (
  savedTheme === "light" ||
  savedTheme === "dark" ||
  savedTheme === "system"
) {
  useAppStore.getState().setTheme(savedTheme);
}
```

从磁盘读到的是不可信的历史数据，即使声明泛型也要运行时校验。复杂设置增加 schema version 和迁移函数。

保存时区分内存更新和磁盘结果：

```typescript
async function persistTheme(nextTheme: ThemeMode): Promise<void> {
  const previous = useAppStore.getState().theme;
  useAppStore.getState().setTheme(nextTheme);
  try {
    const store = await Store.load("settings.json");
    await store.set("theme", nextTheme);
    await store.save();
  } catch (error) {
    useAppStore.getState().setTheme(previous);
    throw error;
  }
}
```

是否回滚即时 UI 取决于产品语义，但绝不能静默宣称保存成功。

## 主题数据流

```text
plugin-store 恢复 ThemeMode
  -> Zustand 保存 light/dark/system
  -> resolveTheme 解析实际 light/dark
  -> document.documentElement data-theme
  -> Ant Design ConfigProvider theme
```

- `system` 模式监听 `prefers-color-scheme` 变化。
- 卸载时清理 listener。
- CSS Variables 与 Ant Design token 使用同一解析结果。
- 初始化时避免先渲染错误主题造成闪烁。

## 防止同步循环

持久化恢复、系统事件、路由同步和用户操作都可能写同一状态。为每条路径明确来源：

- 恢复只在初始化完成前执行一次。
- 外部事件同值时不重复写。
- 写磁盘不触发再次加载。
- setter 不暗含额外副作用。

## 验证

- 无偏好、有效偏好、损坏值和旧版本设置。
- 保存失败和应用强制退出后的行为。
- 刷新/重启恢复、system 模式跟随、暗亮主题组件一致性。
- 使用真实 Tauri 环境验证插件；浏览器 mock 只能作为组件测试。


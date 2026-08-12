# Tauri 文件系统实现模式

仅在实现应用文件读写、导入导出、对话框或拖放时读取。

## Rust 后端模式

文件业务逻辑放 Service，Command 保持薄封装。路径使用 `Path`/`PathBuf`：

```rust
use std::path::{Component, Path, PathBuf};

pub fn read_utf8_text(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput("目标不是普通文件".into()));
    }
    if metadata.len() > max_bytes {
        return Err(AppError::InvalidInput("文件超过允许大小".into()));
    }
    std::fs::read_to_string(path).map_err(AppError::from)
}

pub fn resolve_existing_child(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err(AppError::InvalidInput("只允许相对路径".into()));
    }
    let allowed = root.canonicalize()?;
    // canonicalize 目标本身，确保最终符号链接也不会逃逸。
    let resolved = allowed.join(relative).canonicalize()?;
    if !resolved.starts_with(&allowed) {
        return Err(AppError::InvalidInput("路径超出允许范围".into()));
    }
    Ok(resolved)
}
```

该函数只适用于已存在目标。创建新文件时，需校验路径组件、`canonicalize` 已存在父目录并验证前缀；最终组件使用 no-follow/`create_new` 或目录描述符相对打开等平台安全方式立即创建。路径检查与打开之间存在 TOCTOU，不能把“先检查、稍后按原字符串打开”视为安全授权。符号链接、大小写、UNC 路径和不存在目标按平台测试。

## 原子写入

重要设置或可恢复数据使用同目录临时文件，以保证重命名尽可能原子：

```text
校验目标父目录
  -> 在同目录创建唯一临时文件
  -> 写入并 flush（高风险时 sync_all）
  -> 设置需要的权限
  -> rename 替换目标
  -> 失败时清理临时文件并保留旧文件
```

避免跨文件系统移动临时文件；Windows 上替换已存在文件的语义需按项目支持版本验证。

## Tauri FS Plugin

只在前端直接访问文件确实更合适时使用。先确认依赖、Rust 插件注册和 Capabilities：

```json
{
  "permissions": [
    {
      "identifier": "fs:allow-read-text-file",
      "allow": [{ "path": "$APPDATA/**" }]
    }
  ]
}
```

权限标识和 scope 格式必须按当前插件版本与项目现有配置核对，不照抄示例。

```typescript
import { BaseDirectory, readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";

const content = await readTextFile("config.json", {
  baseDir: BaseDirectory.AppData,
});

await writeTextFile("output.txt", "content", {
  baseDir: BaseDirectory.AppData,
});
```

## 文件对话框

```typescript
import { open, save } from "@tauri-apps/plugin-dialog";

const selected = await open({
  multiple: false,
  directory: false,
  filters: [{ name: "JSON", extensions: ["json"] }],
});
if (selected === null) {
  return; // 用户取消，不作为错误
}

const output = await save({
  defaultPath: "export.json",
  filters: [{ name: "JSON", extensions: ["json"] }],
});
if (output !== null) {
  await fileApi.exportJson(output);
}
```

插件返回类型可能随版本变化，必须读取当前 TypeScript 类型；多选、目录选择和移动端行为分别处理。

## 拖放导入

- 只接受明确文件类型和数量。
- 拖放路径仍视为不可信输入，执行同样的路径、大小和内容校验。
- 大文件解析移到 Rust 异步任务，使用事件/Channel 报告进度，不阻塞 UI。
- 对重复导入、部分成功和用户取消定义清楚反馈。

## 常见路径

应用数据、配置、缓存、日志和用户文档具有不同生命周期。通过 Tauri `app.path()` 或前端 path API 获取对应目录，不默认使用 home/desktop。

```rust
let data_dir = app
    .path()
    .app_data_dir()
    .map_err(|error| AppError::Custom(error.to_string()))?;
```

## 测试矩阵

- 合法文件、缺失文件、目录冒充文件、权限不足。
- `..`、绝对路径、最终/父级符号链接逃逸、检查后替换符号链接竞态、Unicode 文件名、超长路径。
- 空文件、超限文件、非 UTF-8、损坏 JSON/压缩包。
- 对话框取消、多选、重复拖放。
- 写入中断、目标已存在、磁盘满和临时文件清理。

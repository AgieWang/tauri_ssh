# ADR-003：异构文档接入与图谱技术验证

- 状态：待确认（不锁定为桌面应用依赖）
- 日期：2026-08-03
- 适用环境：Apple Silicon macOS 本机
- 关联任务：`refactor-team-knowledge-base-platform` 2.1-2.8

## 已验证证据

| 候选 | 版本 / 许可证 | 本机结果 | 结论 |
| --- | --- | --- | --- |
| `calamine` | 0.36.1 / MIT | `cargo check` 通过 | 作为 XLSX/XLSB/ODS 的只读工作表、单元格、公式和缓存值读取候选；不计算公式 |
| `zip` + `quick-xml` | 6.0.0、0.39.4 / MIT | `cargo check` 通过 | 作为 DOCX/PPTX OOXML 受限读取候选；只读取白名单 XML 条目，不执行宏、外链或嵌入对象 |
| `pdf-extract` | 0.12.0 / MIT | `cargo check` 通过 | 作为文本层 PDF 候选；无文本页必须显式标记“需要 OCR” |
| `ammonia` + `scraper` | 4.1.4（MIT/Apache-2.0）、0.24.0（ISC） | `cargo run --release -- html` 通过 | 可移除 script 与事件属性并保留可见文本；默认**不会**移除 `https` 图片资源，必须叠加协议/资源属性剥离 |
| SQLite FTS5 `trigram` | bundled SQLite 3.45.0 | `cargo run --release -- fts` 通过 | 中文子串“退款审批”仅在 `trigram` 命中；版本号、路径、类名和接口在两种 tokenizer 下均可命中 |
| 本地 OCR | `ocrs` 0.12.2（MIT/Apache-2.0）候选 | 本机未安装 Tesseract、无可用离线 OCR 模型 | 首期只定义可选本地 OCR 适配器与“需要 OCR”状态，不把模型或下载行为加入默认应用 |

本次 Spike 依赖只位于 `scripts/knowledge-spikes/`，未进入 `src-tauri/Cargo.toml`。其 release 二进制为 8.4 MB；编译缓存约 1.7 GB，不应被视为发布包体数据。

## 安全结论

1. HTML/SVG 必须先限制原始字节数、DOM 节点数和嵌套深度，再清洗；清洗后仍须删除 `script/style/iframe/object/embed`、`on*` 属性、`javascript:` URL、外部 `src/href`、SVG 外链与数据 URI。
2. OOXML/ZIP 必须限制条目数、每项及总解压字节数、压缩比和路径；禁止 `..`、绝对路径、重复目录穿越和加密/未知压缩方法。
3. 解析器只接收 Rust 内存或受控临时副本，不启动 Office、浏览器、脚本或网络请求；旧版 DOC/XLS/PPT 默认返回能力错误。
4. OCR 仅在用户选择并验证离线模型目录后启用；远程视觉必须使用独立授权、回环契约测试、固定主机/HTTPS/超时/大小限制。

## 暂定实现取舍

- DOCX：白名单读取 `word/document.xml`、关系与媒体引用，提取标题、段落、列表和表格；不支持对象给出警告。
- XLSX：使用 `calamine`，保留工作表和单元格坐标；公式只保存表达式与库提供的缓存值。
- PPTX：白名单读取 `ppt/slides/slide*.xml` 与备注，按幻灯片顺序生成稳定引用。
- PDF：使用文本层提取；零文本页进入可选 OCR 路径。
- 图谱：首期采用前端原生 SVG/DOM 的有界子图（默认一层、节点和边上限），不在本阶段引入 Canvas/WebGL 第三方依赖；1000/5000 节点基准在图谱页面原型完成后执行。

## 待确认与复审门槛

- 需要实际含中文、表格、图片、损坏、加密、压缩炸弹和大文件夹具后才可锁入主应用依赖。
- 当前没有可用于准确率、延迟、内存、置信度评估的本地 OCR 模型；在模型目录经用户选择或随应用提供前，OCR 保持可选不可用。
- 最终默认限制、解析器版本、标题索引回填和图谱渲染性能阈值，必须以本机夹具基准数据更新本 ADR 后才能转为“已接受”。

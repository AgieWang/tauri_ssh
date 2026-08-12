/**
 * 文档类型的存储值由 Rust 服务写入 SQLite，页面只负责将其转换成可理解的中文。
 * 搜索筛选必须复用同一份定义，避免把展示分组误传成不存在的存储值（例如 office）。
 */
export const knowledgeDocumentTypeLabels: Record<string, string> = {
  markdown: "Markdown 文档",
  rich_text: "手工文档",
  text: "文本资料",
  html: "HTML 原型",
  docx: "Word 文档",
  xlsx: "Excel 表格",
  pptx: "PowerPoint 演示",
  legacy_office: "旧版 Office 文档",
  office: "Office 文档（历史）",
  pdf: "PDF 文档",
  image: "图片",
  requirement: "需求文档",
  code: "代码文件",
  source_code: "代码文件（历史）",
  code_report: "源码分析报告",
  code_analysis: "代码分析（历史）",
  sql: "SQL 脚本",
  json: "JSON 配置",
  yaml: "YAML 配置",
  zentao_report: "禅道同步文档",
  zentao_ai_summary: "禅道 AI 摘要",
  experience: "团队经验",
};

/** 搜索直接使用持久化类型值，label 始终面向非技术用户。 */
export const knowledgeDocumentTypeOptions = Object.entries(
  knowledgeDocumentTypeLabels,
).map(([value, label]) => ({ value, label }));

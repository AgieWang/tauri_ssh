// ─── 类型统一入口（Re-export Hub） ─────────────────────
// 按业务模块拆分类型定义，此处统一导出
//
// 新增模块时：
// 1. 创建 src/types/xxx.ts
// 2. 在此处 export

export type { AppConfig } from "./config";
export type { SystemInfo } from "./system";

// ─── API 统一入口（Re-export Hub） ─────────────────────
// 按业务模块拆分，每个模块独立文件，此处统一导出

// 基础工具（错误解析 + invoke 封装）
export {
  parseCommandError,
  getErrorMessage,
  getErrorCode,
  hasTauriRuntime,
  invoke,
} from "./client";
export type { CommandError } from "./client";

// 业务 API
export { systemApi } from "./system";
export { systemSettingsApi } from "./systemSettings";
export { configApi } from "./config";
export { updaterApi } from "./updater";
export { aiProviderApi } from "./aiProvider";
export { aiSkillApi } from "./aiSkill";
export { sshServerApi } from "./sshServer";
export { credentialVaultApi } from "./credentialVault";
export { secureCredentialApi } from "./secureCredential";
export { databaseOpsApi } from "./databaseOps";
export { terminalApi } from "./terminal";
export { sftpApi } from "./sftp";
export { mcpApi } from "./mcp";
export { approvalApi } from "./approval";
export { jumpserverApi } from "./jumpserver";
export { auditApi } from "./audit";
export { resourceMonitorApi } from "./resourceMonitor";
export { deploymentApi } from "./deployment";

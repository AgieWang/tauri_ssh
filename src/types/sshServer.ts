export type SshServerSource = "manual" | "ssh_config" | "jumpserver";
export type SshServerAuthType = "key" | "password_ref" | "direct_password" | "session_reference";
export type SshServerStatus = "unknown" | "online" | "offline" | "degraded" | "web";
export type SshServerPolicy = "readonly" | "L1" | "L2" | "L3" | "blocked";

export interface SshServer {
  alias: string;
  groupName: string;
  host: string;
  port: number;
  username: string;
  source: SshServerSource;
  authType: SshServerAuthType;
  authRef: string;
  identityFile: string;
  passwordMasked: string | null;
  hasPassword: boolean;
  proxyJump: string;
  aiPolicy: SshServerPolicy;
  status: SshServerStatus;
  enabled: boolean;
  lastConnectedAt: string | null;
  updatedAt: string;
}

export interface UpsertSshServerInput {
  alias: string;
  groupName: string;
  host: string;
  port: number;
  username: string;
  source: SshServerSource;
  authType: SshServerAuthType;
  authRef: string;
  identityFile: string;
  password?: string | null;
  clearPassword?: boolean;
  proxyJump: string;
  aiPolicy: SshServerPolicy;
  status?: SshServerStatus;
  enabled: boolean;
}

export interface SshServerConnectionTestInput {
  alias?: string | null;
  host: string;
  port: number;
}

export interface SshServerTestResult {
  ok: boolean;
  alias: string;
  endpoint: string;
  latencyMs: number;
  message: string;
}

export interface SshConfigImportResult {
  imported: number;
  skipped: number;
  servers: SshServer[];
}

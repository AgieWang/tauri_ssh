export type JumpServerProtocol = "web_ssh" | "web_sftp" | "jumpserver_asset";
export type JumpServerAiMode = "suggest_only" | "disabled";
export type JumpServerStatus = "unknown" | "available" | "opened" | "expired" | "disabled";

export interface JumpServerSession {
  key: string;
  name: string;
  endpoint: string;
  webUrl: string;
  sessionRef: string;
  groupName: string;
  accountHint: string;
  assetHint: string;
  protocol: JumpServerProtocol;
  aiMode: JumpServerAiMode;
  status: JumpServerStatus;
  notes: string;
  enabled: boolean;
  lastOpenedAt: string | null;
  updatedAt: string;
}

export interface UpsertJumpServerSessionInput {
  key: string;
  name: string;
  endpoint: string;
  webUrl: string;
  sessionRef: string;
  groupName: string;
  accountHint: string;
  assetHint: string;
  protocol: JumpServerProtocol;
  aiMode: JumpServerAiMode;
  status?: JumpServerStatus;
  notes: string;
  enabled: boolean;
}

export interface JumpServerOpenResult {
  key: string;
  webUrl: string;
  message: string;
}

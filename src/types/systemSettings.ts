import type { ThemeMode } from "@/store/app";

export type SystemLogLevel = "debug" | "info" | "warn" | "error";
export type SystemPlatform = "macos-windows";
export type SystemCloseBehavior = "minimize" | "exit";
export type SystemLanguage = "zh-CN" | "en-US";

export interface SystemSettings {
  theme: ThemeMode;
  autoUpdate: boolean;
  launchOnStartup: boolean;
  auditRetentionDays: number;
  logLevel: SystemLogLevel;
  backupDir: string;
  databaseDownloadDir: string;
  platform: SystemPlatform;
  closeBehavior: SystemCloseBehavior;
  language: SystemLanguage;
  aiUnrestrictedUntil: string | null;
  dangerousCommands: string[];
}

export interface UpdateSystemSettingsInput extends SystemSettings {}

export interface SystemSettingsExportResult {
  fileName: string;
  content: string;
}

export interface AiUnrestrictedState {
  active: boolean;
  until: string | null;
  remainingSeconds: number;
}

export interface EnableAiUnrestrictedInput {
  minutes?: number;
}

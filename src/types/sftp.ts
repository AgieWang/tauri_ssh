export type SftpFileType = "file" | "directory" | "symlink";

export interface SftpListInput {
  serverAlias: string;
  path: string;
}

export interface SftpFileEntry {
  name: string;
  path: string;
  parent: string;
  fileType: SftpFileType;
  size: number;
  modifiedAt: number | null;
  permissions: string;
  readonly: boolean;
}

export interface SftpListResult {
  serverAlias: string;
  path: string;
  parent: string;
  entries: SftpFileEntry[];
}

export interface SftpReadTextInput {
  serverAlias: string;
  path: string;
  maxBytes?: number | null;
}

export interface SftpReadTextResult {
  serverAlias: string;
  path: string;
  content: string;
  size: number;
  truncated: boolean;
}

export interface SftpWriteTextInput {
  serverAlias: string;
  path: string;
  content: string;
}

export interface SftpTransferPathInput {
  serverAlias: string;
  remotePath: string;
  localPath: string;
}

export interface SftpCreateDirectoryInput {
  serverAlias: string;
  path: string;
}

export interface SftpCreateFileInput {
  serverAlias: string;
  path: string;
  content?: string | null;
}

export interface SftpRenameInput {
  serverAlias: string;
  fromPath: string;
  toPath: string;
}

export interface SftpDeleteInput {
  serverAlias: string;
  path: string;
  fileType: SftpFileType;
}

export interface SftpOperationResult {
  ok: boolean;
  serverAlias: string;
  path: string;
  message: string;
  bytes: number | null;
}

import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  SftpCreateDirectoryInput,
  SftpCreateFileInput,
  SftpDeleteInput,
  SftpListInput,
  SftpListResult,
  SftpOperationResult,
  SftpReadTextInput,
  SftpReadTextResult,
  SftpRenameInput,
  SftpTransferPathInput,
  SftpWriteTextInput,
} from "@/types";

function callSftp<T>(command: string, path: string, input: unknown) {
  return hasTauriRuntime()
    ? invoke<T>(command, { input })
    : devApiFetch<T>(path, {
        method: "POST",
        body: JSON.stringify(input),
      });
}

export const sftpApi = {
  list: (input: SftpListInput) =>
    callSftp<SftpListResult>("sftp_list", "/sftp/list", input),
  readText: (input: SftpReadTextInput) =>
    callSftp<SftpReadTextResult>("sftp_read_text", "/sftp/read-text", input),
  writeText: (input: SftpWriteTextInput) =>
    callSftp<SftpOperationResult>("sftp_write_text", "/sftp/write-text", input),
  upload: (input: SftpTransferPathInput) =>
    callSftp<SftpOperationResult>("sftp_upload", "/sftp/upload", input),
  download: (input: SftpTransferPathInput) =>
    callSftp<SftpOperationResult>("sftp_download", "/sftp/download", input),
  createDirectory: (input: SftpCreateDirectoryInput) =>
    callSftp<SftpOperationResult>(
      "sftp_create_directory",
      "/sftp/create-directory",
      input,
    ),
  createFile: (input: SftpCreateFileInput) =>
    callSftp<SftpOperationResult>(
      "sftp_create_file",
      "/sftp/create-file",
      input,
    ),
  rename: (input: SftpRenameInput) =>
    callSftp<SftpOperationResult>("sftp_rename", "/sftp/rename", input),
  delete: (input: SftpDeleteInput) =>
    callSftp<SftpOperationResult>("sftp_delete", "/sftp/delete", input),
};

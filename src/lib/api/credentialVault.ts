import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  AuthorizeCredentialInput,
  CredentialVaultItem,
  RotateCredentialInput,
  UpsertCredentialInput,
} from "@/types";

export const credentialVaultApi = {
  list: () =>
    hasTauriRuntime()
      ? invoke<CredentialVaultItem[]>("list_credentials")
      : devApiFetch<CredentialVaultItem[]>("/credentials"),
  upsert: (input: UpsertCredentialInput) =>
    hasTauriRuntime()
      ? invoke<CredentialVaultItem>("upsert_credential", { input })
      : devApiFetch<CredentialVaultItem>("/credentials", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  authorize: (input: AuthorizeCredentialInput) =>
    hasTauriRuntime()
      ? invoke<CredentialVaultItem>("authorize_credential", { input })
      : devApiFetch<CredentialVaultItem>("/credentials/authorize", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  rotate: (input: RotateCredentialInput) =>
    hasTauriRuntime()
      ? invoke<CredentialVaultItem>("rotate_credential", { input })
      : devApiFetch<CredentialVaultItem>("/credentials/rotate", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  delete: (key: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_credential", { key })
      : devApiFetch<void>(`/credentials/${encodeURIComponent(key)}`, {
          method: "DELETE",
        }),
};

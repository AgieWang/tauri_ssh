export type CredentialType =
  "private_key" | "password" | "token" | "session_reference" | "api_key";

export type CredentialStatus =
  "normal" | "rotation_due" | "session_reference" | "disabled";

export interface CredentialVaultItem {
  key: string;
  credentialType: CredentialType;
  scope: string;
  status: CredentialStatus;
  description: string;
  secretMasked: string | null;
  hasSecret: boolean;
  enabled: boolean;
  rotatedAt: string | null;
  updatedAt: string;
}

export interface UpsertCredentialInput {
  key: string;
  credentialType: CredentialType;
  scope: string;
  status?: CredentialStatus;
  description: string;
  secret?: string | null;
  clearSecret?: boolean;
  enabled: boolean;
}

export interface AuthorizeCredentialInput {
  key: string;
  scope: string;
}

export interface RotateCredentialInput {
  key: string;
  secret: string;
}

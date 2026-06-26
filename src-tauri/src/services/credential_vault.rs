use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AuthorizeCredentialInput, CredentialVaultItem, RotateCredentialInput, UpsertCredentialInput,
};

const CREDENTIAL_SECRET_SEED_KEY: &str = "credential_vault_secret_seed";

pub struct CredentialVaultService;

impl CredentialVaultService {
    pub fn list(db: &Database) -> Result<Vec<CredentialVaultItem>, AppError> {
        db.list_credentials()
    }

    pub fn upsert(
        db: &Database,
        input: UpsertCredentialInput,
    ) -> Result<CredentialVaultItem, AppError> {
        Self::validate_upsert(&input)?;
        let encrypted_secret = match input
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(secret) => {
                let (nonce, ciphertext) = Self::encrypt_secret(db, secret)?;
                Some((nonce, ciphertext))
            }
            None => None,
        };
        let encrypted_ref = encrypted_secret
            .as_ref()
            .map(|(nonce, ciphertext)| (nonce.as_str(), ciphertext.as_str()));
        db.upsert_credential(&input, encrypted_ref, input.clear_secret.unwrap_or(false))
    }

    pub fn authorize(
        db: &Database,
        input: AuthorizeCredentialInput,
    ) -> Result<CredentialVaultItem, AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭据 Key 不能为空".into()));
        }
        if input.scope.trim().is_empty() {
            return Err(AppError::InvalidInput("授权范围不能为空".into()));
        }
        db.authorize_credential(input.key.trim(), input.scope.trim())
    }

    pub fn rotate(
        db: &Database,
        input: RotateCredentialInput,
    ) -> Result<CredentialVaultItem, AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭据 Key 不能为空".into()));
        }
        if input.secret.trim().is_empty() {
            return Err(AppError::InvalidInput("新凭据内容不能为空".into()));
        }
        let encrypted_secret = Self::encrypt_secret(db, input.secret.trim())?;
        db.rotate_credential(
            input.key.trim(),
            (encrypted_secret.0.as_str(), encrypted_secret.1.as_str()),
        )
    }

    pub fn delete(db: &Database, key: &str) -> Result<(), AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭据 Key 不能为空".into()));
        }
        if !db.delete_credential(key.trim())? {
            return Err(AppError::NotFound(format!("凭据 '{}' 不存在", key)));
        }
        Ok(())
    }

    pub fn get_secret(db: &Database, key: &str) -> Result<String, AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭据 Key 不能为空".into()));
        }
        let row = db
            .get_credential_secret_row(key.trim())?
            .ok_or_else(|| AppError::NotFound(format!("凭据 '{}' 不存在", key)))?;
        match (row.secret_nonce, row.secret_ciphertext) {
            (Some(nonce), Some(ciphertext)) => Self::decrypt_secret(db, &nonce, &ciphertext),
            _ => Err(AppError::InvalidInput(format!(
                "凭据 '{}' 未保存可用密钥内容",
                key
            ))),
        }
    }

    fn validate_upsert(input: &UpsertCredentialInput) -> Result<(), AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭据 Key 不能为空".into()));
        }
        if ![
            "private_key",
            "password",
            "token",
            "session_reference",
            "api_key",
        ]
        .contains(&input.credential_type.as_str())
        {
            return Err(AppError::InvalidInput("凭据类型无效".into()));
        }
        if input.scope.trim().is_empty() {
            return Err(AppError::InvalidInput("授权范围不能为空".into()));
        }
        if !["normal", "rotation_due", "session_reference", "disabled"]
            .contains(&input.status.as_deref().unwrap_or("normal"))
        {
            return Err(AppError::InvalidInput("凭据状态无效".into()));
        }
        let has_new_secret = input
            .secret
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !has_new_secret && input.clear_secret.unwrap_or(false) {
            return Err(AppError::InvalidInput(
                "清空凭据时必须重新提交有效内容".into(),
            ));
        }
        Ok(())
    }

    fn encrypt_secret(db: &Database, secret: &str) -> Result<(String, String), AppError> {
        let key = Self::secret_key(db)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("凭据密钥初始化失败".into()))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, secret.as_bytes())
            .map_err(|_| AppError::Custom("凭据加密失败".into()))?;
        Ok((
            general_purpose::STANDARD.encode(nonce_bytes),
            general_purpose::STANDARD.encode(ciphertext),
        ))
    }

    fn decrypt_secret(db: &Database, nonce: &str, ciphertext: &str) -> Result<String, AppError> {
        let key = Self::secret_key(db)?;
        let nonce_bytes = general_purpose::STANDARD
            .decode(nonce)
            .map_err(|_| AppError::Custom("凭据 nonce 解码失败".into()))?;
        let ciphertext_bytes = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|_| AppError::Custom("凭据密文解码失败".into()))?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("凭据密钥初始化失败".into()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext_bytes.as_ref())
            .map_err(|_| AppError::Custom("凭据解密失败".into()))?;
        String::from_utf8(plaintext).map_err(|_| AppError::Custom("凭据不是有效 UTF-8".into()))
    }

    fn secret_key(db: &Database) -> Result<[u8; 32], AppError> {
        let seed = match db.get_config(CREDENTIAL_SECRET_SEED_KEY)? {
            Some(value) => value,
            None => {
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let value = general_purpose::STANDARD.encode(bytes);
                db.set_config(CREDENTIAL_SECRET_SEED_KEY, &value)?;
                value
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let digest = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest[..32]);
        Ok(key)
    }
}

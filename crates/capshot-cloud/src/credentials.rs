//! Secrets live only in the OS keychain (Credential Manager, Keychain or
//! Secret Service), never in config.json.

use crate::{CloudError, Result};

const SERVICE: &str = "capshot";

pub const R2_ACCESS_KEY_ID: &str = "r2.access_key_id";
pub const R2_SECRET_ACCESS_KEY: &str = "r2.secret_access_key";
pub const GDRIVE_CLIENT_ID: &str = "gdrive.client_id";
pub const GDRIVE_CLIENT_SECRET: &str = "gdrive.client_secret";
pub const GDRIVE_TOKEN: &str = "gdrive.token";

/// Where secrets are kept. Tests use [`MemoryStore`].
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

pub struct Keychain;

impl SecretStore for Keychain {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, key).map_err(|e| CloudError::Keychain(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CloudError::Keychain(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, key)
            .and_then(|e| e.set_password(value))
            .map_err(|e| CloudError::Keychain(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, key).map_err(|e| CloudError::Keychain(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CloudError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests and for running without a keychain.
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<std::collections::HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Fetch a required secret.
pub fn require(store: &dyn SecretStore, key: &str, what: &'static str) -> Result<String> {
    store.get(key)?.filter(|v| !v.is_empty()).ok_or(CloudError::NotConfigured(what))
}

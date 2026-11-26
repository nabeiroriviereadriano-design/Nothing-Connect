//! Coffre de secrets de l'application.
//!
//! * Windows : Credential Manager (chiffré DPAPI pour l'utilisateur courant).
//! * Autres systèmes : **développement uniquement** — fichier par secret, permissions 0600.

use nc_core::SecretStore;

#[cfg(windows)]
pub const SERVICE: &str = "NOTHING CONNECT";

#[cfg(windows)]
pub fn store(_data_dir: &std::path::Path) -> Box<dyn SecretStore> {
    Box::new(nc_security::WindowsCredentialStore { service: SERVICE.into() })
}

#[cfg(not(windows))]
pub fn store(data_dir: &std::path::Path) -> Box<dyn SecretStore> {
    Box::new(dev::DevFileStore { dir: data_dir.join("dev-secrets") })
}

#[cfg(not(windows))]
mod dev {
    use nc_core::{SecretStore, SecurityError};
    use std::path::PathBuf;

    pub struct DevFileStore {
        pub dir: PathBuf,
    }

    impl SecretStore for DevFileStore {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>, SecurityError> {
            match std::fs::read(self.dir.join(name)) {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(SecurityError::Store(e.to_string())),
            }
        }
        fn set(&self, name: &str, value: &[u8]) -> Result<(), SecurityError> {
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir_all(&self.dir).map_err(|e| SecurityError::Store(e.to_string()))?;
            let p = self.dir.join(name);
            std::fs::write(&p, value).map_err(|e| SecurityError::Store(e.to_string()))?;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).map_err(|e| SecurityError::Store(e.to_string()))
        }
        fn delete(&self, name: &str) -> Result<(), SecurityError> {
            let _ = std::fs::remove_file(self.dir.join(name));
            Ok(())
        }
    }
}

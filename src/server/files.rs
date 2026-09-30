use std::{
    io,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// Content-addressed file store under `DATA_DIR/files/ab/abcdef…`; identical uploads share one file.
pub struct FileStore {
    root: PathBuf,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

impl FileStore {
    pub fn new(data_dir: &Path) -> Self {
        Self { root: data_dir.join("files") }
    }

    fn path(&self, sha256: &str) -> Option<PathBuf> {
        (sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.root.join(&sha256[..2]).join(sha256))
    }

    pub async fn put(&self, bytes: &[u8]) -> io::Result<String> {
        let sha = sha256_hex(bytes);
        let path = self.path(&sha).expect("sha256 hex is a valid path");
        if tokio::fs::try_exists(&path).await? {
            return Ok(sha);
        }
        tokio::fs::create_dir_all(path.parent().expect("has parent")).await?;
        let tmp = path.with_extension(format!("tmp-{}", crate::server::session::random_token()));
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(sha)
    }

    pub async fn get(&self, sha256: &str) -> io::Result<Vec<u8>> {
        let path = self
            .path(sha256)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file id"))?;
        tokio::fs::read(path).await
    }
}

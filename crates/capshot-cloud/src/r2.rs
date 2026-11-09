//! Cloudflare R2 uploads through the S3 API (presigned PUT), with winshot's
//! object key, retry and public URL rules.

use crate::credentials::{self, SecretStore};
use crate::{check_size, content_type, http_client, http_err, CloudError, Result};
use capshot_core::config::R2Config;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use std::time::Duration;

const MAX_RETRIES: u32 = 3;
const RETRY_BASE: Duration = Duration::from_millis(500);
const SIGN_FOR: Duration = Duration::from_secs(300);

pub struct R2 {
    config: R2Config,
    access_key: String,
    secret_key: String,
    /// Override for tests; production uses `https://<account>.r2.cloudflarestorage.com`.
    endpoint: Option<String>,
}

impl R2 {
    pub fn from_store(config: &R2Config, store: &dyn SecretStore) -> Result<Self> {
        if config.account_id.is_empty() || config.bucket.is_empty() || config.public_url.is_empty() {
            return Err(CloudError::NotConfigured("Cloudflare R2"));
        }
        Ok(Self {
            config: config.clone(),
            access_key: credentials::require(store, credentials::R2_ACCESS_KEY_ID, "R2 access key")?,
            secret_key: credentials::require(store, credentials::R2_SECRET_ACCESS_KEY, "R2 secret key")?,
            endpoint: None,
        })
    }

    #[cfg(test)]
    fn with_endpoint(mut self, url: &str) -> Self {
        self.endpoint = Some(url.to_string());
        self
    }

    fn bucket(&self) -> Result<Bucket> {
        let endpoint = self
            .endpoint
            .clone()
            .unwrap_or_else(|| format!("https://{}.r2.cloudflarestorage.com", self.config.account_id));
        let url = endpoint.parse().map_err(|e: url::ParseError| CloudError::Other(e.to_string()))?;
        Bucket::new(url, UrlStyle::Path, self.config.bucket.clone(), "auto")
            .map_err(|e| CloudError::Other(e.to_string()))
    }

    /// `directory/filename`, with the directory's slashes trimmed.
    pub fn object_key(&self, filename: &str) -> String {
        let dir = self.config.directory.trim_matches('/');
        if dir.is_empty() {
            filename.to_string()
        } else {
            format!("{dir}/{filename}")
        }
    }

    /// Public URL: base URL + percent-encoded key, keeping `/` separators.
    pub fn public_url(&self, key: &str) -> String {
        let encoded: Vec<String> = key.split('/').map(percent_encode).collect();
        format!("{}/{}", self.config.public_url.trim_end_matches('/'), encoded.join("/"))
    }

    /// Upload and return the public URL. Retries transient failures with
    /// exponential backoff (1 s, 2 s).
    pub fn upload(&self, data: &[u8], filename: &str) -> Result<String> {
        check_size(data)?;
        let bucket = self.bucket()?;
        let creds = Credentials::new(&self.access_key, &self.secret_key);
        let key = self.object_key(filename);
        let client = http_client()?;
        let mut last = CloudError::Other("upload did not run".into());
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                std::thread::sleep(RETRY_BASE * (1 << attempt));
            }
            let url = bucket.put_object(Some(&creds), &key).sign(SIGN_FOR);
            match client.put(url).header("Content-Type", content_type(filename)).body(data.to_vec()).send() {
                Ok(r) if r.status().is_success() => return Ok(self.public_url(&key)),
                Ok(r) => {
                    let status = r.status().as_u16();
                    last = CloudError::Status { service: "R2", status, body: r.text().unwrap_or_default() };
                    if (400..500).contains(&status) && status != 429 {
                        break; // credentials or bucket problem: retrying will not help
                    }
                }
                Err(e) => last = http_err(e),
            }
        }
        Err(last)
    }

    /// Check the credentials can reach the bucket (HEAD bucket).
    pub fn test_connection(&self) -> Result<()> {
        let bucket = self.bucket()?;
        let creds = Credentials::new(&self.access_key, &self.secret_key);
        let url = bucket.head_bucket(Some(&creds)).sign(SIGN_FOR);
        let r = http_client()?.head(url).send().map_err(http_err)?;
        if r.status().is_success() {
            Ok(())
        } else {
            Err(CloudError::Status { service: "R2", status: r.status().as_u16(), body: String::new() })
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryStore;
    use crate::mock;

    fn r2(dir: &str) -> R2 {
        let store = MemoryStore::default();
        store.set(credentials::R2_ACCESS_KEY_ID, "AKID").unwrap();
        store.set(credentials::R2_SECRET_ACCESS_KEY, "SECRET").unwrap();
        let cfg = R2Config {
            account_id: "acc".into(),
            bucket: "shots".into(),
            public_url: "https://cdn.example.com/".into(),
            directory: dir.into(),
        };
        R2::from_store(&cfg, &store).unwrap()
    }

    #[test]
    fn missing_config_or_secrets_is_reported() {
        let store = MemoryStore::default();
        assert!(matches!(R2::from_store(&R2Config::default(), &store), Err(CloudError::NotConfigured(_))));
        let cfg = R2Config {
            account_id: "a".into(),
            bucket: "b".into(),
            public_url: "https://x".into(),
            directory: String::new(),
        };
        assert!(matches!(R2::from_store(&cfg, &store), Err(CloudError::NotConfigured(_))));
    }

    #[test]
    fn keys_and_public_urls() {
        assert_eq!(r2("").object_key("a.png"), "a.png");
        assert_eq!(r2("/screens/2026/").object_key("a.png"), "screens/2026/a.png");
        assert_eq!(r2("").public_url("dir/my shot.png"), "https://cdn.example.com/dir/my%20shot.png");
    }

    #[test]
    fn upload_signs_and_puts() {
        let m = mock::serve(vec![(200, "")]);
        let url = r2("shots").with_endpoint(&m.url).upload(b"PNGDATA", "x.png").unwrap();
        assert_eq!(url, "https://cdn.example.com/shots/x.png");
        let req = &m.requests.lock().unwrap()[0];
        assert_eq!(req.method, "PUT");
        assert!(req.path.starts_with("/shots/shots/x.png?"), "{}", req.path);
        assert!(req.path.contains("X-Amz-Signature="));
        assert_eq!(req.header("content-type"), Some("image/png"));
        assert_eq!(req.body, b"PNGDATA");
    }

    #[test]
    fn server_errors_are_retried_but_auth_errors_are_not() {
        let m = mock::serve(vec![(500, "oops"), (200, "")]);
        assert!(r2("").with_endpoint(&m.url).upload(b"x", "x.png").is_ok());
        assert_eq!(m.requests.lock().unwrap().len(), 2);

        let m = mock::serve(vec![(403, "denied"), (200, "")]);
        let err = r2("").with_endpoint(&m.url).upload(b"x", "x.png").unwrap_err();
        assert!(matches!(err, CloudError::Status { status: 403, .. }));
        assert_eq!(m.requests.lock().unwrap().len(), 1);
    }
}

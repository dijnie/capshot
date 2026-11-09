//! Cloud services for capshot: Cloudflare R2 and Google Drive uploads, secrets
//! in the OS keychain, and update checks. All calls block; run them off the UI
//! thread.

pub mod credentials;
pub mod gdrive;
pub mod r2;
pub mod updater;

#[cfg(test)]
mod mock;

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("{0} is not configured")]
    NotConfigured(&'static str),
    #[error("file is {0} bytes; the limit is 50 MB")]
    TooLarge(usize),
    #[error("nothing to upload")]
    Empty,
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("network: {0}")]
    Http(String),
    #[error("{service} answered {status}: {body}")]
    Status { service: &'static str, status: u16, body: String },
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CloudError>;

pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

/// Content type from a file name (winshot's `detectContentType`).
pub fn content_type(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn check_size(data: &[u8]) -> Result<()> {
    if data.is_empty() {
        return Err(CloudError::Empty);
    }
    if data.len() > MAX_UPLOAD_BYTES {
        return Err(CloudError::TooLarge(data.len()));
    }
    Ok(())
}

/// Network error text without the request URL: presigned R2 URLs carry the
/// access key ID and a live signature, and error text reaches the UI.
fn http_err(e: reqwest::Error) -> CloudError {
    CloudError::Http(e.without_url().to_string())
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(concat!("capshot/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(http_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_match_winshot() {
        assert_eq!(content_type("a.PNG"), "image/png");
        assert_eq!(content_type("a.jpeg"), "image/jpeg");
        assert_eq!(content_type("a.tif"), "image/tiff");
        assert_eq!(content_type("a"), "application/octet-stream");
    }

    #[test]
    fn size_limits() {
        assert!(matches!(check_size(&[]), Err(CloudError::Empty)));
        assert!(check_size(&[1]).is_ok());
    }
}

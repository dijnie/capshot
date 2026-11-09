//! OS services for capshot. Every module exposes plain functions and data so
//! the UI never touches platform APIs directly.

pub mod autostart;
pub mod capture;
pub mod clipboard;
pub mod files;
pub mod hotkeys;
pub use capshot_core::icon;
pub mod notify;
pub mod single_instance;
pub mod tray;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("screen capture failed: {0}")]
    Capture(String),
    #[error("clipboard: {0}")]
    Clipboard(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, PlatformError>;

//! App configuration. The JSON shape matches winshot's `config.json` (camelCase
//! keys) so an existing winshot config imports unchanged.

use crate::crop::CropAspect;
use crate::geometry::OutputRatio;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HotkeyConfig {
    pub fullscreen: String,
    pub region: String,
    pub window: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            fullscreen: "PrintScreen".into(),
            region: "Ctrl+PrintScreen".into(),
            window: "Ctrl+Shift+PrintScreen".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StartupConfig {
    pub launch_on_startup: bool,
    pub minimize_to_tray: bool,
    pub show_notification: bool,
    pub close_to_tray: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self { launch_on_startup: false, minimize_to_tray: false, show_notification: true, close_to_tray: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilenamePattern {
    #[default]
    Timestamp,
    Date,
    Increment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuickSaveConfig {
    pub folder: String,
    pub pattern: FilenamePattern,
}

impl Default for QuickSaveConfig {
    fn default() -> Self {
        Self { folder: default_save_folder().to_string_lossy().into_owned(), pattern: FilenamePattern::Timestamp }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    #[serde(alias = "jpg")]
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportConfig {
    pub default_format: ImageFormat,
    pub jpeg_quality: u8,
    pub include_background: bool,
    pub auto_copy_to_clipboard: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            default_format: ImageFormat::Png,
            jpeg_quality: 95,
            include_background: true,
            auto_copy_to_clipboard: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { width: 1200, height: 800 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderType {
    Outside,
    #[default]
    Center,
    Inside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EditorConfig {
    pub padding: u32,
    pub corner_radius: u32,
    pub shadow_size: u32,
    pub background_color: String,
    pub output_ratio: OutputRatio,
    pub show_background: bool,
    pub inset: u32,
    pub auto_background: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inset_background_color: Option<String>,
    pub shape_corner_radius: u32,
    pub border_enabled: bool,
    pub border_weight: u32,
    pub border_color: String,
    pub border_opacity: u32,
    pub border_type: BorderType,
    /// capshot addition: remembered crop preset. Ignored by winshot.
    pub crop_aspect: CropAspect,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            padding: 40,
            corner_radius: 12,
            shadow_size: 20,
            background_color: crate::presets::DEFAULT_BACKGROUND.into(),
            output_ratio: OutputRatio::Auto,
            show_background: true,
            inset: 0,
            auto_background: true,
            inset_background_color: None,
            shape_corner_radius: 0,
            border_enabled: false,
            border_weight: 2,
            border_color: "#000000".into(),
            border_opacity: 100,
            border_type: BorderType::Center,
            crop_aspect: CropAspect::Free,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateConfig {
    pub check_on_startup: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub skipped_version: String,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { check_on_startup: true, skipped_version: String::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct R2Config {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub account_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bucket: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub public_url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub directory: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GDriveConfig {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub folder_id: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CloudConfig {
    pub r2: R2Config,
    pub gdrive: GDriveConfig,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub hotkeys: HotkeyConfig,
    pub startup: StartupConfig,
    pub quick_save: QuickSaveConfig,
    pub export: ExportConfig,
    pub window: WindowConfig,
    pub editor: EditorConfig,
    pub update: UpdateConfig,
    pub cloud: CloudConfig,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub background_images: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no config directory on this system")]
    NoConfigDir,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// How the loaded config was obtained, so the UI can mention an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadSource {
    Existing,
    ImportedFromWinshot(PathBuf),
    Defaults,
    /// The file was unreadable JSON; defaults are used and the file was copied
    /// to `config.json.bak` before anything overwrites it.
    InvalidFile(String),
}

impl Config {
    /// `<config dir>/capshot/config.json`.
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        directories::ProjectDirs::from("", "", "capshot")
            .map(|d| d.config_dir().join("config.json"))
            .ok_or(ConfigError::NoConfigDir)
    }

    /// Where winshot keeps its config (`%APPDATA%\WinShot\config.json` on Windows).
    pub fn winshot_path() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.config_dir().join("WinShot").join("config.json"))
    }

    /// Load from `path`. When it does not exist, import `winshot` (read only)
    /// if given and present, otherwise start from defaults. Either way the
    /// result is written to `path`.
    pub fn load_or_init(path: &Path, winshot: Option<&Path>) -> Result<(Self, LoadSource), ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                return Ok(match serde_json::from_str(&text) {
                    Ok(cfg) => (cfg, LoadSource::Existing),
                    Err(e) => {
                        let backup = path.with_extension("json.bak");
                        std::fs::copy(path, &backup)?;
                        (Self::default(), LoadSource::InvalidFile(format!("{e}; saved a copy as {}", backup.display())))
                    }
                })
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            Err(_) => {}
        }
        let imported = winshot
            .filter(|p| p.is_file())
            .and_then(|p| Some((serde_json::from_str::<Config>(&std::fs::read_to_string(p).ok()?).ok()?, p)));
        let (cfg, source) = match imported {
            Some((cfg, p)) => (cfg, LoadSource::ImportedFromWinshot(p.to_path_buf())),
            None => (Self::default(), LoadSource::Defaults),
        };
        cfg.save(path)?;
        Ok((cfg, source))
    }

    /// Write atomically: a temp file next to the target, then rename.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn default_save_folder() -> PathBuf {
    let base = directories::UserDirs::new()
        .and_then(|d| d.picture_dir().map(Path::to_path_buf))
        .or_else(|| directories::BaseDirs::new().map(|d| d.home_dir().join("Pictures")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("capshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINSHOT_SAMPLE: &str = r##"{
      "hotkeys": {"fullscreen": "PrintScreen", "region": "Ctrl+Alt+R", "window": "Ctrl+Shift+PrintScreen"},
      "startup": {"launchOnStartup": true, "minimizeToTray": true, "showNotification": false, "closeToTray": false},
      "quickSave": {"folder": "C:\\Users\\me\\Pictures\\WinShot", "pattern": "increment"},
      "export": {"defaultFormat": "jpeg", "jpegQuality": 80, "includeBackground": false, "autoCopyToClipboard": false},
      "window": {"width": 1400, "height": 900},
      "editor": {"padding": 64, "cornerRadius": 8, "shadowSize": 10,
        "backgroundColor": "linear-gradient(135deg, #f093fb 0%, #f5576c 100%)", "outputRatio": "16:9",
        "showBackground": true, "inset": 10, "autoBackground": false, "insetBackgroundColor": "#112233",
        "shapeCornerRadius": 6, "borderEnabled": true, "borderWeight": 3, "borderColor": "#ff0000",
        "borderOpacity": 50, "borderType": "inside"},
      "update": {"checkOnStartup": false, "skippedVersion": "1.5.0"},
      "cloud": {"r2": {"accountId": "acc", "bucket": "b", "publicUrl": "https://x.r2.dev", "directory": "shots"},
                "gdrive": {"folderId": "fid"}},
      "backgroundImages": ["data:image/png;base64,AAAA"],
      "someFutureField": 1
    }"##;

    #[test]
    fn winshot_config_imports_every_field() {
        let c: Config = serde_json::from_str(WINSHOT_SAMPLE).unwrap();
        assert_eq!(c.hotkeys.region, "Ctrl+Alt+R");
        assert!(c.startup.launch_on_startup && c.startup.minimize_to_tray);
        assert!(!c.startup.show_notification && !c.startup.close_to_tray);
        assert_eq!(c.quick_save.pattern, FilenamePattern::Increment);
        assert_eq!(c.export.default_format, ImageFormat::Jpeg);
        assert_eq!(c.export.jpeg_quality, 80);
        assert!(!c.export.include_background && !c.export.auto_copy_to_clipboard);
        assert_eq!((c.window.width, c.window.height), (1400, 900));
        let e = &c.editor;
        assert_eq!((e.padding, e.corner_radius, e.shadow_size, e.inset), (64, 8, 10, 10));
        assert_eq!(e.output_ratio, OutputRatio::R16x9);
        assert_eq!(e.inset_background_color.as_deref(), Some("#112233"));
        assert_eq!((e.shape_corner_radius, e.border_weight, e.border_opacity), (6, 3, 50));
        assert!(e.border_enabled && !e.auto_background);
        assert_eq!(e.border_type, BorderType::Inside);
        assert_eq!(c.update.skipped_version, "1.5.0");
        assert_eq!(c.cloud.r2.public_url, "https://x.r2.dev");
        assert_eq!(c.cloud.gdrive.folder_id, "fid");
        assert_eq!(c.background_images.len(), 1);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let c: Config = serde_json::from_str(r#"{"editor": {"padding": 5}}"#).unwrap();
        assert_eq!(c.editor.padding, 5);
        assert_eq!(c.editor.corner_radius, 12);
        assert_eq!(c.hotkeys, HotkeyConfig::default());
    }

    #[test]
    fn serialises_with_winshot_key_names() {
        let v = serde_json::to_value(Config::default()).unwrap();
        assert!(v["quickSave"]["folder"].is_string());
        assert_eq!(v["export"]["jpegQuality"], 95);
        assert_eq!(v["editor"]["borderType"], "center");
        assert_eq!(v["editor"]["outputRatio"], "auto");
    }

    #[test]
    fn load_imports_winshot_once_without_touching_it() {
        let dir = tempfile::tempdir().unwrap();
        let winshot = dir.path().join("WinShot/config.json");
        std::fs::create_dir_all(winshot.parent().unwrap()).unwrap();
        std::fs::write(&winshot, WINSHOT_SAMPLE).unwrap();
        let path = dir.path().join("capshot/config.json");

        let (c, src) = Config::load_or_init(&path, Some(&winshot)).unwrap();
        assert_eq!(src, LoadSource::ImportedFromWinshot(winshot.clone()));
        assert_eq!(c.editor.padding, 64);
        assert_eq!(std::fs::read_to_string(&winshot).unwrap(), WINSHOT_SAMPLE);

        let (again, src) = Config::load_or_init(&path, Some(&winshot)).unwrap();
        assert_eq!(src, LoadSource::Existing);
        assert_eq!(again, c);
    }

    #[test]
    fn invalid_file_falls_back_to_defaults_and_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        let (c, src) = Config::load_or_init(&path, None).unwrap();
        assert!(matches!(src, LoadSource::InvalidFile(_)));
        assert_eq!(c, Config::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
        assert_eq!(std::fs::read_to_string(path.with_extension("json.bak")).unwrap(), "{not json");
    }
}

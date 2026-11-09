//! Icons. The component library embeds ~100 icons; the editor's extra Lucide
//! icons are embedded here and looked up first.

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(
    AppIcons,
    [
        MousePointer2,
        Square,
        Circle,
        MoveUpRight,
        Minus,
        Type,
        Hash,
        Crop,
        Undo2,
        Redo2,
        Trash,
        Monitor,
        SquareDashed,
        AppWindow,
        Clipboard,
        FolderOpen,
        Settings,
        Images,
        Save,
        Copy,
        Flashlight,
        Download,
        Spline,
        ZoomIn,
        ZoomOut,
        Maximize,
        PaintBucket,
        Check,
        X,
        RotateCcw
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = AppIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(AppIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

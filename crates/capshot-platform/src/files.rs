//! File operations that should go through the desktop (recycle bin / trash).

pub fn move_to_trash(path: &std::path::Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

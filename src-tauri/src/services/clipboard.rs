use crate::error::{AppError, AppResult};

pub fn read_text() -> AppResult<String> {
    Ok(island_windows::clipboard::read_text()?.unwrap_or_default())
}

pub fn write_text(text: &str) -> AppResult<()> {
    island_windows::clipboard::write_text(text).map_err(AppError::from)
}

pub fn has_image() -> bool {
    island_windows::clipboard::has_image()
}

pub fn read_file_paths() -> AppResult<Vec<String>> {
    island_windows::clipboard::read_file_paths().map_err(AppError::from)
}

//! Clipboard utilities with HTML-to-Markdown conversion support.

#[cfg(not(target_os = "android"))]
use clipboard_rs::{common::RustImage, Clipboard as ClipboardTrait, ClipboardContext, ContentFormat};
use htmd::{
    element_handler::Handlers,
    options::{BulletListMarker, Options},
    Element, HtmlToMarkdown,
};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "android"))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

pub use crate::editor::{Clipboard, ClipboardError, ClipboardResult, MemoryClipboard};

pub enum ClipboardContent {
    Markdown(String),
    PlainText(String),
    Empty,
}

pub enum ClipboardImage {
    Png(Vec<u8>),
    Files(Vec<PathBuf>),
}

#[derive(Debug, Default)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn set_text(&self, text: &str) -> ClipboardResult<()> {
        set_system_text_result(text)
    }
    fn get_text(&self) -> ClipboardResult<Option<String>> {
        get_system_text_result()
    }
    fn get_html(&self) -> ClipboardResult<Option<String>> {
        get_system_html_result()
    }
    fn get_image_png(&self) -> ClipboardResult<Option<Vec<u8>>> {
        get_system_image_png_result()
    }
    fn get_files(&self) -> ClipboardResult<Vec<PathBuf>> {
        get_system_files_result()
    }
}

pub fn default_clipboard() -> Arc<dyn Clipboard> {
    #[cfg(test)]
    {
        Arc::new(MemoryClipboard::default())
    }
    #[cfg(not(test))]
    {
        Arc::new(SystemClipboard)
    }
}

/// Reuse one context: X11 starts an ownership thread for each context and a
/// per-operation context both leaks threads and corrupts the TUI on handoff.
#[cfg(not(target_os = "android"))]
fn clipboard_context() -> Option<&'static Mutex<ClipboardContext>> {
    static CTX: OnceLock<Option<Mutex<ClipboardContext>>> = OnceLock::new();
    CTX.get_or_init(|| native_clipboard_available().then(|| ClipboardContext::new().ok().map(Mutex::new)).flatten()).as_ref()
}

#[cfg(target_os = "macos")]
fn native_clipboard_available() -> bool {
    // clipboard-rs assumes `generalPasteboard` is non-null and panics in
    // headless sessions, so probe the same service before entering the crate.
    std::process::Command::new("/usr/bin/pbpaste").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|status| status.success())
}

#[cfg(all(not(target_os = "android"), not(target_os = "macos")))]
const fn native_clipboard_available() -> bool {
    true
}

#[cfg(not(target_os = "android"))]
fn with_clipboard<T>(f: impl FnOnce(&ClipboardContext) -> T) -> Option<T> {
    let guard = clipboard_context()?.lock().ok()?;
    Some(f(&guard))
}

#[cfg(not(target_os = "android"))]
fn set_system_text_result(text: &str) -> ClipboardResult<()> {
    with_clipboard(|ctx| ctx.set_text(text.to_string()).map_err(|error| ClipboardError::ReadError(error.to_string()))).unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn set_system_text_result(_text: &str) -> ClipboardResult<()> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn has_html() -> bool {
    with_clipboard(|ctx| ctx.has(ContentFormat::Html)).unwrap_or(false)
}

#[cfg(target_os = "android")]
pub fn has_html() -> bool {
    false
}

#[cfg(not(target_os = "android"))]
fn get_system_html_result() -> ClipboardResult<Option<String>> {
    with_clipboard(|ctx| {
        if !ctx.has(ContentFormat::Html) {
            return Ok(None);
        }
        ctx.get_html().map(Some).map_err(|e| ClipboardError::ReadError(e.to_string()))
    })
    .unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn get_system_html_result() -> ClipboardResult<Option<String>> {
    Ok(None)
}

#[cfg(not(target_os = "android"))]
fn get_system_text_result() -> ClipboardResult<Option<String>> {
    with_clipboard(|ctx| ctx.get_text().map(Some).map_err(|e| ClipboardError::ReadError(e.to_string()))).unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn get_system_text_result() -> ClipboardResult<Option<String>> {
    Ok(None)
}

#[cfg(not(target_os = "android"))]
fn get_system_image_png_result() -> ClipboardResult<Option<Vec<u8>>> {
    with_clipboard(|ctx| {
        if !ctx.has(ContentFormat::Image) {
            return Ok(None);
        }
        let image = ctx.get_image().map_err(|error| ClipboardError::ReadError(error.to_string()))?;
        let png = image.to_png().map_err(|error| ClipboardError::ConversionError(error.to_string()))?;
        Ok(Some(png.get_bytes().to_vec()))
    })
    .unwrap_or(Ok(None))
}

#[cfg(target_os = "android")]
fn get_system_image_png_result() -> ClipboardResult<Option<Vec<u8>>> {
    Ok(None)
}

#[cfg(not(target_os = "android"))]
fn get_system_files_result() -> ClipboardResult<Vec<PathBuf>> {
    with_clipboard(|ctx| {
        if !ctx.has(ContentFormat::Files) {
            return Ok(Vec::new());
        }
        ctx.get_files().map(|files| files.iter().map(|file| file_entry_path(file)).collect()).map_err(|error| ClipboardError::ReadError(error.to_string()))
    })
    .unwrap_or(Ok(Vec::new()))
}

#[cfg(target_os = "android")]
fn get_system_files_result() -> ClipboardResult<Vec<PathBuf>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "android"))]
fn file_entry_path(entry: &str) -> PathBuf {
    let Some(uri_path) = entry.strip_prefix("file://") else { return PathBuf::from(entry) };
    let uri_path = uri_path.strip_prefix("localhost").unwrap_or(uri_path);
    let bytes = uri_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%').then(|| uri_path.get(index + 1..index + 3)).flatten().and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
}

fn is_image_file(path: &Path) -> bool {
    path.is_file() && image::ImageFormat::from_path(path).is_ok_and(|format| format.reading_enabled())
}

pub fn get_image_from(clipboard: &dyn Clipboard) -> ClipboardResult<Option<ClipboardImage>> {
    let files = clipboard.get_files().unwrap_or_default();
    if !files.is_empty() {
        return Ok(files.iter().all(|path| is_image_file(path)).then_some(ClipboardImage::Files(files)));
    }
    if clipboard.get_text().ok().flatten().is_some_and(|text| !text.trim().is_empty()) {
        return Ok(None);
    }
    Ok(clipboard.get_image_png()?.map(ClipboardImage::Png))
}
fn create_converter() -> HtmlToMarkdown {
    let options = Options { bullet_list_marker: BulletListMarker::Dash, ..Options::default() };
    HtmlToMarkdown::builder()
        .options(options)
        .add_handler(vec!["a"], |handlers: &dyn Handlers, element: Element| {
            let mut href: Option<String> = None;
            for attr in element.attrs.iter() {
                if &*attr.name.local == "href" {
                    href = Some(attr.value.to_string());
                    break;
                }
            }
            let href = match href {
                Some(h) if !h.is_empty() => h,
                _ => return Some(handlers.walk_children(element.node)),
            };
            if href.starts_with('#') {
                return Some(handlers.walk_children(element.node));
            }
            let content = handlers.walk_children(element.node).content;
            let text = content.trim();
            if text.is_empty() {
                return None;
            }
            let href = href.replace('(', "\\(").replace(')', "\\)");
            Some(format!("[{}]({})", text, href).into())
        })
        .build()
}

/// Convert HTML to Markdown using htmd with custom link handling
pub fn html_to_markdown(html: &str) -> ClipboardResult<String> {
    let converter = create_converter();
    converter.convert(html).map_err(|e| ClipboardError::ConversionError(e.to_string()))
}

/// Get clipboard content, converting HTML to Markdown if available
///
/// Priority:
/// 1. If HTML is available, convert to Markdown
/// 2. Fall back to plain text
/// 3. Return Empty if nothing available
pub fn get_content_as_markdown() -> ClipboardResult<ClipboardContent> {
    get_content_as_markdown_from(&SystemClipboard)
}

pub fn get_content_as_markdown_from(clipboard: &dyn Clipboard) -> ClipboardResult<ClipboardContent> {
    if let Ok(Some(html)) = clipboard.get_html() {
        if !html.trim().is_empty() {
            if let Ok(md) = html_to_markdown(&html) {
                let trimmed = md.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(ClipboardContent::Markdown(trimmed));
                }
            }
        }
    }
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}

pub fn get_content_plain() -> ClipboardResult<ClipboardContent> {
    get_content_plain_from(&SystemClipboard)
}

pub fn get_content_plain_from(clipboard: &dyn Clipboard) -> ClipboardResult<ClipboardContent> {
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_clipboard_content_is_converted_before_plain_text() {
        let clipboard = MemoryClipboard::with_text("fallback");
        clipboard.set_html("<p>Hello <strong>world</strong></p>");
        let ClipboardContent::Markdown(markdown) = get_content_as_markdown_from(&clipboard).unwrap() else { panic!("expected Markdown") };
        assert_eq!(markdown, "Hello **world**");
    }

    #[test]
    fn image_data_pastes_only_when_the_clipboard_has_no_text() {
        let clipboard = MemoryClipboard::default();
        clipboard.set_image_png(b"png".to_vec());
        assert!(matches!(get_image_from(&clipboard).unwrap(), Some(ClipboardImage::Png(png)) if png == b"png"));
        clipboard.set_text("caption").unwrap();
        assert!(get_image_from(&clipboard).unwrap().is_none());
        assert!(get_image_from(&MemoryClipboard::default()).unwrap().is_none());
    }

    #[test]
    fn copied_files_paste_as_images_only_when_every_file_is_an_image() {
        let root = std::env::temp_dir().join(format!("ekphos-clipboard-files-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let photo = root.join("photo.png");
        let notes = root.join("notes.txt");
        std::fs::write(&photo, b"png").unwrap();
        std::fs::write(&notes, b"text").unwrap();
        let clipboard = MemoryClipboard::with_text("photo.png");
        clipboard.set_image_png(b"icon".to_vec());
        clipboard.set_files([photo.clone()]);
        assert!(matches!(get_image_from(&clipboard).unwrap(), Some(ClipboardImage::Files(files)) if files == [photo.clone()]));
        clipboard.set_files([photo, notes]);
        assert!(get_image_from(&clipboard).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn file_uris_become_decoded_paths() {
        assert_eq!(file_entry_path("/Users/me/shot.png"), PathBuf::from("/Users/me/shot.png"));
        assert_eq!(file_entry_path("file:///home/me/Pasted%20image.png"), PathBuf::from("/home/me/Pasted image.png"));
        assert_eq!(file_entry_path("file://localhost/home/me/caf%C3%A9.png"), PathBuf::from("/home/me/café.png"));
        assert_eq!(file_entry_path("file:///home/me/100%.png"), PathBuf::from("/home/me/100%.png"));
    }

    #[test]
    fn plain_and_empty_clipboards_keep_their_behavior() {
        let clipboard = MemoryClipboard::with_text("plain");
        assert!(matches!(get_content_as_markdown_from(&clipboard).unwrap(), ClipboardContent::PlainText(text) if text == "plain"));
        assert!(matches!(get_content_plain_from(&MemoryClipboard::default()).unwrap(), ClipboardContent::Empty));
    }
}

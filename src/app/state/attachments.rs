use super::*;
use crate::clipboard::ClipboardImage;
use crate::vault::attachments;

impl App {
    pub fn clipboard_image_link(&self) -> Option<Result<String, String>> {
        match crate::clipboard::get_image_from(self.clipboard()) {
            Ok(Some(image)) => Some(self.save_clipboard_image(image).map_err(|error| format!("Couldn't paste image: {error}"))),
            Ok(None) => None,
            Err(error) => Some(Err(format!("Clipboard: {error}"))),
        }
    }

    fn save_clipboard_image(&self, image: ClipboardImage) -> Result<String, String> {
        let notes_dir = self.state.config.notes_path();
        let note_dir = self.current_note().and_then(|note| note.file_path.as_deref()).and_then(std::path::Path::parent).map_or_else(|| notes_dir.clone(), std::path::Path::to_path_buf);
        let dir = attachments::attachment_dir(&notes_dir, &note_dir, &self.state.config.attachments_dir)?;
        let paths = match image {
            ClipboardImage::Png(png) => vec![attachments::save_attachment(&dir, &attachments::pasted_image_name(self.dependencies.clock.local_now()), &png)?],
            ClipboardImage::Files(files) => files.iter().map(|file| attachments::import_file(&notes_dir, &dir, file)).collect::<Result<_, _>>()?,
        };
        Ok(paths.iter().map(|path| attachments::markdown_image_link(&note_dir, path)).collect::<Vec<_>>().join("\n"))
    }
}

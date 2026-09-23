use chrono::NaiveDateTime;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

pub fn attachment_dir(notes_dir: &Path, note_dir: &Path, configured: &str) -> Result<PathBuf, String> {
    let configured = configured.trim();
    let (base, relative) = match configured.strip_prefix("./") {
        Some(relative) => (note_dir, relative),
        None if configured == "." => (note_dir, ""),
        None => (notes_dir, configured),
    };
    let relative = Path::new(relative);
    if relative.is_absolute() || relative.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err("attachments_dir must be a relative path inside the notes directory".to_string());
    }
    Ok(base.join(relative))
}

pub fn pasted_image_name(now: NaiveDateTime) -> String {
    format!("Pasted image {}.png", now.format("%Y%m%d%H%M%S"))
}

pub fn save_attachment(dir: &Path, file_name: &str, contents: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("failed to create directory {}: {error}", dir.display()))?;
    let name = Path::new(file_name);
    let stem = name.file_stem().and_then(|stem| stem.to_str()).unwrap_or(file_name);
    let extension = name.extension().and_then(|extension| extension.to_str());
    let mut index = 0usize;
    loop {
        let candidate = match (index, extension) {
            (0, _) => file_name.to_string(),
            (_, Some(extension)) => format!("{stem} {index}.{extension}"),
            (_, None) => format!("{stem} {index}"),
        };
        let path = dir.join(candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                return match file.write_all(contents) {
                    Ok(()) => Ok(path),
                    Err(error) => {
                        let _ = fs::remove_file(&path);
                        Err(format!("failed to write {}: {error}", path.display()))
                    }
                };
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => index += 1,
            Err(error) => return Err(format!("failed to create {}: {error}", path.display())),
        }
    }
}

pub fn import_file(notes_dir: &Path, dir: &Path, source: &Path) -> Result<PathBuf, String> {
    if source.starts_with(notes_dir) {
        return Ok(source.to_path_buf());
    }
    let file_name = source.file_name().and_then(|name| name.to_str()).ok_or_else(|| format!("unsupported file name: {}", source.display()))?;
    let contents = fs::read(source).map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    save_attachment(dir, file_name, &contents)
}

pub fn markdown_image_link(note_dir: &Path, target: &Path) -> String {
    let destination = match relative_path(note_dir, target) {
        Some(relative) => relative.components().map(|component| component.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
        None => target.to_string_lossy().into_owned(),
    };
    format!("![]({})", encode_destination(&destination))
}

fn relative_path(from_dir: &Path, target: &Path) -> Option<PathBuf> {
    let from: Vec<Component> = from_dir.components().collect();
    let to: Vec<Component> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
    if common == 0 {
        return None;
    }
    let mut relative: PathBuf = from[common..].iter().map(|_| Component::ParentDir).collect();
    relative.extend(&to[common..]);
    Some(relative)
}

fn encode_destination(destination: &str) -> String {
    let mut encoded = String::with_capacity(destination.len());
    for character in destination.chars() {
        match character {
            ' ' | '%' | '(' | ')' | '<' | '>' | '[' | ']' => {
                let _ = write!(encoded, "%{:02X}", character as u32);
            }
            character if character.is_ascii_control() => {
                let _ = write!(encoded, "%{:02X}", character as u32);
            }
            character => encoded.push(character),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("ekphos-attachments-{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn attachment_dir_is_vault_relative_unless_it_starts_with_dot_slash() {
        let vault = Path::new("/vault");
        let note_dir = Path::new("/vault/Projects/Alpha");
        assert_eq!(attachment_dir(vault, note_dir, "attachments").unwrap(), PathBuf::from("/vault/attachments"));
        assert_eq!(attachment_dir(vault, note_dir, " Media/Images ").unwrap(), PathBuf::from("/vault/Media/Images"));
        assert_eq!(attachment_dir(vault, note_dir, "").unwrap(), PathBuf::from("/vault"));
        assert_eq!(attachment_dir(vault, note_dir, ".").unwrap(), PathBuf::from("/vault/Projects/Alpha"));
        assert_eq!(attachment_dir(vault, note_dir, "./").unwrap(), PathBuf::from("/vault/Projects/Alpha"));
        assert_eq!(attachment_dir(vault, note_dir, "./assets").unwrap(), PathBuf::from("/vault/Projects/Alpha/assets"));
    }

    #[test]
    fn attachment_dir_rejects_locations_outside_the_vault() {
        let vault = Path::new("/vault");
        for configured in ["..", "../images", "attachments/../../x", "./../images", "/tmp/images"] {
            assert!(attachment_dir(vault, vault, configured).is_err(), "{configured}");
        }
    }

    #[test]
    fn pasted_image_names_follow_obsidian() {
        let now = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(8, 5, 9).unwrap();
        assert_eq!(pasted_image_name(now), "Pasted image 20260923080509.png");
    }

    #[test]
    fn saving_never_overwrites_an_existing_attachment() {
        let workspace = TempWorkspace::new("unique");
        let dir = workspace.path.join("attachments");
        let first = save_attachment(&dir, "Pasted image.png", b"first").unwrap();
        let second = save_attachment(&dir, "Pasted image.png", b"second").unwrap();
        let third = save_attachment(&dir, "Pasted image.png", b"third").unwrap();
        assert_eq!(first, dir.join("Pasted image.png"));
        assert_eq!(second, dir.join("Pasted image 1.png"));
        assert_eq!(third, dir.join("Pasted image 2.png"));
        assert_eq!(fs::read(first).unwrap(), b"first");
        assert_eq!(fs::read(second).unwrap(), b"second");
    }

    #[test]
    fn importing_copies_outside_files_and_links_vault_files_in_place() {
        let workspace = TempWorkspace::new("import");
        let vault = workspace.path.join("vault");
        let dir = vault.join("attachments");
        let outside = workspace.path.join("photo.jpg");
        let inside = vault.join("existing.png");
        fs::create_dir_all(&vault).unwrap();
        fs::write(&outside, b"jpeg").unwrap();
        fs::write(&inside, b"png").unwrap();
        let copied = import_file(&vault, &dir, &outside).unwrap();
        assert_eq!(copied, dir.join("photo.jpg"));
        assert_eq!(fs::read(&copied).unwrap(), b"jpeg");
        assert!(outside.exists());
        assert_eq!(import_file(&vault, &dir, &inside).unwrap(), inside);
        assert!(!dir.join("existing.png").exists());
    }

    #[test]
    fn links_are_relative_to_the_note_and_escape_destination_syntax() {
        let note_dir = Path::new("/vault/Projects/Alpha");
        assert_eq!(markdown_image_link(note_dir, Path::new("/vault/Projects/Alpha/shot.png")), "![](shot.png)");
        assert_eq!(markdown_image_link(note_dir, Path::new("/vault/attachments/Pasted image 20260923080509.png")), "![](../../attachments/Pasted%20image%2020260923080509.png)");
        assert_eq!(markdown_image_link(note_dir, Path::new("/vault/Projects/Alpha/a (1) [x] 100%.png")), "![](a%20%281%29%20%5Bx%5D%20100%25.png)");
        assert_eq!(markdown_image_link(note_dir, Path::new("/vault/Projects/Alpha/café.png")), "![](café.png)");
    }
}

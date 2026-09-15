use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ekphos-{label}-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_ekphos(argument: &str, environment: &str, path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ekphos")).arg(argument).env(environment, path).output().unwrap()
}

#[test]
fn reset_returns_failure_and_omits_success_message_when_config_delete_fails() {
    let root = TestDir::new("reset-delete-failure");
    let config_dir = root.path().join("config");
    fs::create_dir_all(config_dir.join("config.toml")).unwrap();

    let output = run_ekphos("--reset", "EKPHOS_CONFIG_DIR", &config_dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(!stdout.contains("Reset complete!"));
    assert!(!stdout.contains("  Created:"));
    assert!(stderr.contains("failed to remove config"), "unexpected stderr: {stderr}");
}

#[test]
fn clean_cache_returns_failure_and_omits_success_message_when_delete_fails() {
    let root = TestDir::new("cache-delete-failure");
    let cache_path = root.path().join("cache-file");
    fs::write(&cache_path, "not a directory").unwrap();

    let output = run_ekphos("--clean-cache", "EKPHOS_CACHE_DIR", &cache_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(!stdout.contains("Cache cleared!"));
    assert!(stderr.contains("failed to remove cache"), "unexpected stderr: {stderr}");
}

#[cfg(unix)]
#[test]
fn reset_returns_failure_and_omits_success_message_when_defaults_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDir::new("reset-write-failure");
    let config_dir = root.path().join("config");
    fs::create_dir(&config_dir).unwrap();
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let output = run_ekphos("--reset", "EKPHOS_CONFIG_DIR", &config_dir);
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700)).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(!stdout.contains("Reset complete!"));
    assert!(!stdout.contains("  Created:"));
    assert!(stderr.contains("failed to create themes directory"), "unexpected stderr: {stderr}");
}

#[test]
fn reset_reports_success_only_after_defaults_are_written() {
    let root = TestDir::new("reset-success");
    let config_dir = root.path().join("config");
    let themes_dir = config_dir.join("themes");
    fs::create_dir_all(&themes_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "old config").unwrap();
    fs::write(themes_dir.join("custom.toml"), "old theme").unwrap();

    let output = run_ekphos("--reset", "EKPHOS_CONFIG_DIR", &config_dir);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Reset complete!"));
    assert!(config_dir.join("config.toml").is_file());
    assert!(themes_dir.join("ekphos-dawn.toml").is_file());
    assert!(!themes_dir.join("custom.toml").exists());
}

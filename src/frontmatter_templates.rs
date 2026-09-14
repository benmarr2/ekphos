use crate::config::Config;
use chrono::NaiveDate;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

struct TemplateContext<'a> {
    title: &'a str,
    date: String,
    folder: &'a str,
}

pub(crate) fn initial_note_content(mappings: &BTreeMap<String, String>, config_dir: &Path, vault_root: &Path, destination: &Path, title: &str, date: NaiveDate) -> Result<String, String> {
    if mappings.is_empty() {
        return Ok(plain_note_content(title));
    }
    let parent = destination.parent().ok_or_else(|| "Could not determine the note's destination folder".to_string())?;
    let folder = vault_relative_folder(vault_root, parent)?;
    let Some(template_name) = matching_template(mappings, &folder) else {
        return Ok(plain_note_content(title));
    };
    let template_path = confined_template_path(config_dir, template_name)?;
    let source = fs::read_to_string(&template_path).map_err(|error| format!("Frontmatter template '{template_name}' could not be read: {error}"))?;
    let context = TemplateContext { title, date: date.format("%Y-%m-%d").to_string(), folder: &folder };
    let yaml = render_yaml(template_name, &source, &context)?;
    Ok(format!("---\n{}\n---\n\n# {}\n\n", yaml.trim_end(), title))
}

fn plain_note_content(title: &str) -> String {
    format!("# {title}\n\n")
}

fn vault_relative_folder(vault_root: &Path, folder: &Path) -> Result<String, String> {
    let relative = folder.strip_prefix(vault_root).map_err(|_| "The note's destination folder is outside the vault".to_string())?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err("The note's destination folder is invalid".to_string());
        };
        parts.push(part.to_string_lossy().into_owned());
    }
    Ok(if parts.is_empty() { ".".to_string() } else { parts.join("/") })
}

fn matching_template<'a>(mappings: &'a BTreeMap<String, String>, folder: &str) -> Option<&'a str> {
    let mut candidate = folder;
    loop {
        if let Some(template) = mappings.get(candidate) {
            return Some(template.as_str());
        }
        if candidate == "." {
            return None;
        }
        candidate = candidate.rsplit_once('/').map_or(".", |(parent, _)| parent);
    }
}

fn confined_template_path(config_dir: &Path, template_name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(template_name);
    if template_name.trim().is_empty() || relative.is_absolute() || relative.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err(format!("Frontmatter template path '{template_name}' must stay inside the templates directory"));
    }
    Ok(Config::templates_dir_in(config_dir).join(relative))
}

fn render_yaml(template_name: &str, source: &str, context: &TemplateContext<'_>) -> Result<String, String> {
    let mut value: Value = serde_yaml::from_str(source).map_err(|error| format!("Frontmatter template '{template_name}' is invalid YAML: {error}"))?;
    if !value.is_mapping() {
        return Err(format!("Frontmatter template '{template_name}' must contain a YAML mapping"));
    }
    expand_value(&mut value, context)?;
    reject_placeholders(&value)?;
    let rendered = serde_yaml::to_string(&value).map_err(|error| format!("Frontmatter template '{template_name}' could not be rendered: {error}"))?;
    crate::vault::Frontmatter::parse(&format!("---\n{}\n---\n", rendered.trim_end())).0.ok_or_else(|| format!("Frontmatter template '{template_name}' has fields that Ekphos cannot read"))?;
    Ok(rendered.strip_prefix("---\n").unwrap_or(&rendered).trim_end().to_string())
}

fn expand_value(value: &mut Value, context: &TemplateContext<'_>) -> Result<(), String> {
    match value {
        Value::String(text) => {
            *text = text.replace("{{title}}", context.title).replace("{{date}}", &context.date).replace("{{folder}}", context.folder);
        }
        Value::Sequence(values) => {
            for value in values {
                expand_value(value, context)?;
            }
        }
        Value::Mapping(mapping) => {
            for value in mapping.values_mut() {
                expand_value(value, context)?;
            }
        }
        Value::Tagged(tagged) => expand_value(&mut tagged.value, context)?,
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn reject_placeholders(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) if text.contains("{{") || text.contains("}}") => {
            return Err(format!("Unknown or malformed frontmatter template placeholder in '{text}'"));
        }
        Value::Sequence(values) => {
            for value in values {
                reject_placeholders(value)?;
            }
        }
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if matches!(key, Value::String(text) if text.contains("{{") || text.contains("}}")) {
                    return Err("Frontmatter template placeholders are only supported in YAML values".to_string());
                }
                reject_placeholders(value)?;
            }
        }
        Value::Tagged(tagged) => reject_placeholders(&tagged.value)?,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        config: PathBuf,
        vault: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-frontmatter-templates-{}-{id}", std::process::id()));
            let config = root.join("config");
            let vault = root.join("vault");
            fs::create_dir_all(Config::templates_dir_in(&config)).unwrap();
            fs::create_dir_all(vault.join("Projects/2026")).unwrap();
            Self { root, config, vault }
        }

        fn write_template(&self, name: &str, content: &str) {
            let path = Config::templates_dir_in(&self.config).join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 10).unwrap()
    }

    #[test]
    fn nearest_ancestor_wins_and_placeholders_are_yaml_safe() {
        let fixture = Fixture::new();
        fixture.write_template("root.yaml", "kind: root\n");
        fixture.write_template("project.yaml", "title: \"{{title}}\"\ncreated: \"{{date}}\"\nfolder: \"Folder: {{folder}}\"\ntags: [project]\n");
        let mappings = BTreeMap::from([(".".to_string(), "root.yaml".to_string()), ("Projects".to_string(), "project.yaml".to_string())]);
        let content = initial_note_content(&mappings, &fixture.config, &fixture.vault, &fixture.vault.join("Projects/2026/Research: Q4.md"), "Research: Q4", date()).unwrap();
        let (frontmatter, content_start) = crate::vault::Frontmatter::parse(&content);
        let frontmatter = frontmatter.unwrap();
        assert_eq!(frontmatter.title.as_deref(), Some("Research: Q4"));
        assert_eq!(frontmatter.tags, ["project"]);
        assert_eq!(frontmatter.extra["created"].as_str(), Some("2026-09-10"));
        assert_eq!(frontmatter.extra["folder"].as_str(), Some("Folder: Projects/2026"));
        assert_eq!(content.lines().nth(content_start), Some(""));
        assert!(content.ends_with("# Research: Q4\n\n"));
    }

    #[test]
    fn exact_mapping_overrides_an_ancestor_and_root_is_a_fallback() {
        let mappings = BTreeMap::from([(".".to_string(), "root.yaml".to_string()), ("Projects".to_string(), "project.yaml".to_string()), ("Projects/2026".to_string(), "year.yaml".to_string())]);
        assert_eq!(matching_template(&mappings, "Projects/2026"), Some("year.yaml"));
        assert_eq!(matching_template(&mappings, "Projects/2025"), Some("project.yaml"));
        assert_eq!(matching_template(&mappings, "Personal"), Some("root.yaml"));
    }

    #[test]
    fn no_mapping_keeps_the_existing_plain_note() {
        let fixture = Fixture::new();
        let content = initial_note_content(&BTreeMap::new(), &fixture.config, &fixture.vault, &fixture.vault.join("Plain.md"), "Plain", date()).unwrap();
        assert_eq!(content, "# Plain\n\n");
    }

    #[test]
    fn invalid_templates_and_paths_are_rejected() {
        let fixture = Fixture::new();
        fixture.write_template("invalid.yaml", "tags: not-a-list\n");
        fixture.write_template("malformed.yaml", "tags: [\n");
        fixture.write_template("sequence.yaml", "- not\n- a mapping\n");
        fixture.write_template("unknown.yaml", "value: \"{{unknown}}\"\n");
        for (template, expected) in [("invalid.yaml", "cannot read"), ("malformed.yaml", "invalid YAML"), ("sequence.yaml", "YAML mapping"), ("unknown.yaml", "Unknown or malformed"), ("../escape.yaml", "must stay inside")] {
            let mappings = BTreeMap::from([(".".to_string(), template.to_string())]);
            let error = initial_note_content(&mappings, &fixture.config, &fixture.vault, &fixture.vault.join("Note.md"), "Note", date()).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }
}

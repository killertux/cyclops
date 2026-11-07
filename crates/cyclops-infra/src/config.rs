//! Reads the settings files.

use cyclops_domain::settings::{Layer, Origin, RawValue, SettingsError};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Where your own settings live, and how messages name that place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserConfig {
    pub path: PathBuf,
    pub label: String,
}

impl UserConfig {
    /// `$XDG_CONFIG_HOME/cyclops/config.toml` when that is set to an absolute path, and
    /// otherwise `~/.config/cyclops/config.toml`.
    #[must_use]
    pub fn locate(home: Option<&Path>, xdg_config_home: Option<&Path>) -> Option<Self> {
        if let Some(xdg) = xdg_config_home.filter(|path| path.is_absolute()) {
            let path = xdg.join("cyclops").join("config.toml");
            let label = path.display().to_string();
            return Some(Self { path, label });
        }
        let path = home?.join(".config").join("cyclops").join("config.toml");
        Some(Self {
            path,
            label: "~/.config/cyclops/config.toml".to_owned(),
        })
    }
}

fn raw(value: &toml::Value) -> RawValue {
    match value {
        toml::Value::Boolean(value) => RawValue::Bool(*value),
        toml::Value::Integer(value) => RawValue::Integer(*value),
        toml::Value::String(value) => RawValue::Text(value.clone()),
        toml::Value::Array(items) => RawValue::List(items.iter().map(raw).collect()),
        toml::Value::Float(_) | toml::Value::Datetime(_) | toml::Value::Table(_) => RawValue::Other,
    }
}

/// Lays a table out as `section.name` settings. Tables only nest a few levels, so this
/// recursion is shallow.
fn flatten(prefix: &str, table: &toml::Table, layer: &mut Layer) {
    for (name, value) in table {
        let key = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            toml::Value::Table(inner) => flatten(&key, inner, layer),
            toml::Value::Array(rules) if key == "alerts" => {
                for rule in rules {
                    let fields = match rule {
                        toml::Value::Table(fields) => fields
                            .iter()
                            .map(|(name, value)| (name.clone(), raw(value)))
                            .collect(),
                        _ => vec![(String::new(), RawValue::Other)],
                    };
                    layer.alerts.push(fields);
                }
            }
            other => layer.values.push((key, raw(other))),
        }
    }
}

/// Reads one settings file. A missing file sets nothing.
///
/// # Errors
///
/// Fails if the file cannot be read, or is not TOML.
pub fn read_layer(
    path: &Path,
    origin: Origin,
    label: &str,
) -> Result<Option<Layer>, SettingsError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(SettingsError::Unreadable(label.to_owned())),
    };
    let table: toml::Table = text
        .parse()
        .map_err(|_| SettingsError::NotToml(label.to_owned()))?;
    let mut layer = Layer::new(origin);
    flatten("", &table, &mut layer);
    Ok(Some(layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(name: &str, text: &str) -> Layer {
        let path =
            std::env::temp_dir().join(format!("cyclops-config-{}-{name}.toml", std::process::id()));
        fs::write(&path, text).expect("written");
        let layer = read_layer(&path, Origin::ProjectFile, "file").expect("read");
        fs::remove_file(&path).expect("removed");
        layer.expect("a layer")
    }

    #[test]
    fn sections_become_dotted_names() {
        let layer = layer(
            "sections",
            "[metrics.duplication]\nmin_tokens = 3\n[tests]\ninclude = true\n",
        );
        assert_eq!(
            layer.values,
            [
                (
                    "metrics.duplication.min_tokens".to_owned(),
                    RawValue::Integer(3)
                ),
                ("tests.include".to_owned(), RawValue::Bool(true)),
            ]
        );
    }

    #[test]
    fn alert_rules_are_kept_apart() {
        let layer = layer(
            "alerts",
            "[[alerts]]\nmetric = \"lcom\"\n[[alerts]]\nmetric = \"cyclomatic\"\n",
        );
        assert_eq!(layer.alerts.len(), 2);
        assert_eq!(layer.values, []);
    }

    #[test]
    fn a_missing_file_sets_nothing() {
        let path = Path::new("/no/such/config.toml");
        assert_eq!(read_layer(path, Origin::UserFile, "file"), Ok(None));
    }

    #[test]
    fn your_own_file_follows_xdg_only_when_it_is_absolute() {
        let home = Path::new("/home/a");
        let default = UserConfig::locate(Some(home), None).expect("a place");
        assert_eq!(default.label, "~/.config/cyclops/config.toml");
        assert_eq!(default.path, home.join(".config/cyclops/config.toml"));
        let relative = UserConfig::locate(Some(home), Some(Path::new("other")));
        assert_eq!(relative, Some(default));
        let moved = UserConfig::locate(Some(home), Some(Path::new("/x"))).expect("a place");
        assert_eq!(moved.path, Path::new("/x/cyclops/config.toml"));
    }
}

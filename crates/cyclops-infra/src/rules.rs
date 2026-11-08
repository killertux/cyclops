//! Writing alert rules into a settings file, keeping every other line and comment.

use cyclops_domain::settings::RawValue;
use std::fs;
use std::path::Path;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, value};

fn read(path: &Path) -> Result<DocumentMut, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("{} cannot be read: {error}", path.display())),
    };
    text.parse()
        .map_err(|error| format!("{} is not valid TOML: {error}", path.display()))
}

fn write(path: &Path, document: &DocumentMut) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|error| format!("{} cannot be created: {error}", folder.display()))?;
    }
    fs::write(path, document.to_string())
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))
}

fn item_of(raw: &RawValue) -> Item {
    match raw {
        RawValue::Text(text) => value(text.as_str()),
        RawValue::Bool(flag) => value(*flag),
        RawValue::Integer(number) => value(*number),
        RawValue::List(items) => {
            let mut array = Array::new();
            for item in items {
                if let RawValue::Text(text) = item {
                    array.push(text.as_str());
                }
            }
            value(array)
        }
        RawValue::Other => Item::None,
    }
}

fn rules(document: &mut DocumentMut) -> Result<&mut ArrayOfTables, String> {
    document
        .entry("alerts")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| "alerts is not a list of rules".to_owned())
}

/// Saves a rule into a settings file: in place of the rule at `position`, or as a new one
/// at the end.
///
/// # Errors
///
/// Fails if the file cannot be read, understood or written, or has no rule there.
pub fn save_alert(
    path: &Path,
    position: Option<usize>,
    fields: &[(String, RawValue)],
) -> Result<(), String> {
    let mut document = read(path)?;
    let list = rules(&mut document)?;
    let mut table = Table::new();
    for (name, raw) in fields {
        table.insert(name, item_of(raw));
    }
    match position {
        Some(position) => {
            let existing = list
                .get_mut(position)
                .ok_or_else(|| format!("There is no rule {position} to change."))?;
            existing.clear();
            for (name, item) in &table {
                existing.insert(name, item.clone());
            }
        }
        None => list.push(table),
    }
    write(path, &document)
}

/// Turns a rule of a settings file on or off.
///
/// # Errors
///
/// Fails if the file cannot be read, understood or written, or has no rule there.
pub fn set_alert_enabled(path: &Path, position: usize, enabled: bool) -> Result<(), String> {
    let mut document = read(path)?;
    let rule = rules(&mut document)?
        .get_mut(position)
        .ok_or_else(|| format!("There is no rule {position} to change."))?;
    rule.insert("enabled", value(enabled));
    write(path, &document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("cyclops-rules-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).expect("a folder");
        folder
    }

    fn fields(when: &str) -> Vec<(String, RawValue)> {
        vec![
            ("metric".to_owned(), RawValue::Text("cyclomatic".to_owned())),
            ("scope".to_owned(), RawValue::Text("function".to_owned())),
            ("when".to_owned(), RawValue::Text(when.to_owned())),
            ("severity".to_owned(), RawValue::Text("warning".to_owned())),
        ]
    }

    #[test]
    fn a_new_rule_goes_at_the_end_and_comments_stay() {
        let folder = folder("new");
        let path = folder.join("config.toml");
        fs::write(&path, "# keep this\n[tests]\ninclude = false\n").expect("written");
        save_alert(&path, None, &fields("> 1")).expect("saved");
        let text = fs::read_to_string(&path).expect("read");
        fs::remove_dir_all(&folder).expect("removed");
        assert!(
            text.starts_with("# keep this\n[tests]\ninclude = false\n"),
            "{text}"
        );
        assert!(
            text.contains("[[alerts]]\nmetric = \"cyclomatic\""),
            "{text}"
        );
        assert!(text.contains("when = \"> 1\""), "{text}");
    }

    #[test]
    fn a_rule_is_changed_or_turned_off_in_place() {
        let folder = folder("change");
        let path = folder.join("config.toml");
        save_alert(&path, None, &fields("> 1")).expect("saved");
        save_alert(&path, None, &fields("> 5")).expect("saved");
        save_alert(&path, Some(0), &fields("> 2")).expect("changed");
        set_alert_enabled(&path, 1, false).expect("turned off");
        let text = fs::read_to_string(&path).expect("read");
        let missing = save_alert(&path, Some(9), &fields("> 1"));
        fs::remove_dir_all(&folder).expect("removed");
        let first = text.find("when = \"> 2\"").expect("the first rule changed");
        let second = text.find("when = \"> 5\"").expect("the second rule stays");
        assert!(first < second, "{text}");
        assert!(!text.contains("> 1"), "{text}");
        assert!(text.contains("enabled = false"), "{text}");
        assert!(missing.is_err());
    }
}

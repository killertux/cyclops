//! The environment a command runs in. It is a value rather than the process's own, so
//! that a caller such as the specification can give each run an environment of its own.

use std::collections::HashMap;
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    variables: HashMap<String, String>,
}

impl Environment {
    /// The variables of this process. Values that are not valid UTF-8 are left out, as
    /// no setting could use them.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            variables: env::vars_os()
                .filter_map(|(name, value)| {
                    Some((name.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn new(variables: HashMap<String, String>) -> Self {
        Self { variables }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }

    #[must_use]
    pub fn home(&self) -> Option<&Path> {
        self.get("HOME").map(Path::new)
    }

    #[must_use]
    pub fn xdg_config_home(&self) -> Option<&Path> {
        self.get("XDG_CONFIG_HOME").map(Path::new)
    }
}

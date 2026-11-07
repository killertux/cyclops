//! Path patterns, as settings and filters write them: `*` is any part of a name, `?` is
//! one character, and `**` is any number of folders. A pattern that matches a folder
//! matches everything inside it.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    pattern: String,
}

/// Whether a name matches a pattern of `*` and `?`. It keeps the last `*` to backtrack
/// to, so it never recurses.
fn matches_name(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(c) if *c == '?' || Some(c) == name.get(n) => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((star_p, star_n)) => {
                    p = star_p + 1;
                    n = star_n + 1;
                    star = Some((star_p, star_n + 1));
                }
                None => return false,
            },
        }
    }
    pattern.iter().skip(p).all(|c| *c == '*')
}

/// Whether the parts of a pattern match the parts of a path. `**` can take any number
/// of parts, which the table of what matched so far handles without recursing.
fn matches_parts(pattern: &[&str], path: &[&str]) -> bool {
    // reached[j] says whether the pattern so far can end right after path part j.
    let mut reached = vec![false; path.len() + 1];
    if let Some(first) = reached.first_mut() {
        *first = true;
    }
    for part in pattern {
        let mut next = vec![false; path.len() + 1];
        if *part == "**" {
            let mut any = false;
            for (j, slot) in next.iter_mut().enumerate() {
                any = any || reached.get(j).copied().unwrap_or(false);
                *slot = any;
            }
        } else {
            for (j, name) in path.iter().enumerate() {
                if reached.get(j).copied().unwrap_or(false)
                    && matches_name(part, name)
                    && let Some(slot) = next.get_mut(j + 1)
                {
                    *slot = true;
                }
            }
        }
        reached = next;
    }
    reached.last().copied().unwrap_or(false)
}

impl Glob {
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.trim_matches('/').to_owned(),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Whether a path, relative to the project, matches, or is inside a folder that does.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let pattern: Vec<&str> = self.pattern.split('/').collect();
        let parts: Vec<&str> = path.split('/').collect();
        (1..=parts.len()).any(|end| {
            parts
                .get(..end)
                .is_some_and(|start| matches_parts(&pattern, start))
        })
    }
}

impl fmt::Display for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        Glob::new(pattern).matches(path)
    }

    #[test]
    fn a_star_stays_inside_a_name() {
        assert!(matches("src/notes.*", "src/notes.jsx"));
        assert!(!matches("src/*.rs", "src/a/b.rs"));
        assert!(!matches("*.rs", "src/a.rs"));
    }

    #[test]
    fn two_stars_cross_folders() {
        assert!(matches("src/**/*.rs", "src/a/b/c.rs"));
        assert!(matches("src/**/*.rs", "src/c.rs"));
        assert!(matches("e2e/**", "e2e/a.ts"));
        assert!(matches("**/generated/**", "a/b/generated/c.ts"));
    }

    #[test]
    fn a_folder_matches_what_is_inside_it() {
        assert!(matches("tests", "tests/a/b.rs"));
        assert!(matches("src/notes.rs", "src/notes.rs"));
        assert!(!matches("src/notes.rs", "src/notes.rs.bak"));
    }

    #[test]
    fn a_question_mark_is_one_character() {
        assert!(matches("m0?.rs", "m01.rs"));
        assert!(!matches("m0?.rs", "m001.rs"));
    }

    #[test]
    fn many_stars_do_not_take_forever() {
        let pattern = "*a".repeat(30);
        assert!(!matches(&pattern, &"a".repeat(29)));
    }
}

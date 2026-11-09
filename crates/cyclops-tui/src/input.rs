//! What the person does, independent of any terminal library, and the Vim notation the
//! specification writes it in.

/// One key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    /// Control and a letter.
    Ctrl(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

/// What a mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Click,
    /// The button held down while the mouse moves.
    Drag,
    /// The button let go.
    Release,
    DoubleClick,
    RightClick,
    Hover,
    WheelUp,
    WheelDown,
    CtrlWheelUp,
    CtrlWheelDown,
}

/// Something that reaches the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    Mouse {
        kind: MouseKind,
        column: u16,
        row: u16,
    },
    Resize {
        columns: u16,
        rows: u16,
    },
    /// Time has passed: the milliseconds since the interface opened.
    Tick(u64),
    /// Files of the project changed since the last analysis.
    FilesChanged,
}

/// One gesture of the specification's `keys` column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gesture {
    Key(Key),
    /// The mouse over something named by a target, such as `symbol shop::orders`.
    Mouse {
        kind: MouseKind,
        target: String,
    },
    /// Dragging what a target names by some columns, such as a divider.
    Drag {
        target: String,
        columns: i16,
    },
    Wait(u64),
    Resize {
        columns: u16,
        rows: u16,
    },
    /// A step of the specification, taken while the interface is open.
    Step(String),
    Resume,
}

/// Reads `COLUMNSxROWS`.
#[must_use]
pub fn parse_size(text: &str) -> Option<(u16, u16)> {
    let (columns, rows) = text.trim().split_once('x')?;
    Some((columns.parse().ok()?, rows.parse().ok()?))
}

fn named(name: &str) -> Result<Gesture, String> {
    let key = |key| Ok(Gesture::Key(key));
    match name {
        "Space" => return key(Key::Char(' ')),
        "CR" | "Enter" => return key(Key::Enter),
        "Esc" => return key(Key::Esc),
        "Tab" => return key(Key::Tab),
        "BS" => return key(Key::Backspace),
        "Up" => return key(Key::Up),
        "Down" => return key(Key::Down),
        "Left" => return key(Key::Left),
        "Right" => return key(Key::Right),
        "lt" => return key(Key::Char('<')),
        "resume" => return Ok(Gesture::Resume),
        "wheel up" => return Ok(mouse(MouseKind::WheelUp, "")),
        "wheel down" => return Ok(mouse(MouseKind::WheelDown, "")),
        "ctrl-wheel up" => return Ok(mouse(MouseKind::CtrlWheelUp, "")),
        "ctrl-wheel down" => return Ok(mouse(MouseKind::CtrlWheelDown, "")),
        _ => {}
    }
    if let Some(letter) = name.strip_prefix("C-") {
        let mut letters = letter.chars();
        if let (Some(letter), None) = (letters.next(), letters.next()) {
            return key(Key::Ctrl(letter.to_ascii_lowercase()));
        }
    }
    let (word, rest) = name.split_once(' ').unwrap_or((name, ""));
    match word {
        "wait" => rest
            .parse()
            .map(Gesture::Wait)
            .map_err(|_| format!("not a time: {rest}")),
        "resize" => parse_size(rest)
            .map(|(columns, rows)| Gesture::Resize { columns, rows })
            .ok_or_else(|| format!("not a size: {rest}")),
        "step" => Ok(Gesture::Step(rest.to_owned())),
        "click" => Ok(mouse(MouseKind::Click, rest)),
        "double-click" => Ok(mouse(MouseKind::DoubleClick, rest)),
        "right-click" => Ok(mouse(MouseKind::RightClick, rest)),
        "hover" => Ok(mouse(MouseKind::Hover, rest)),
        "wheel" => match rest.split_once(' ') {
            Some(("up", target)) => Ok(mouse(MouseKind::WheelUp, target)),
            Some(("down", target)) => Ok(mouse(MouseKind::WheelDown, target)),
            _ => Err(format!("not a wheel: {rest}")),
        },
        "drag" => {
            let (target, columns) = rest
                .rsplit_once(' ')
                .ok_or_else(|| format!("not a drag: {rest}"))?;
            let columns = columns
                .parse()
                .map_err(|_| format!("not a distance: {columns}"))?;
            Ok(Gesture::Drag {
                target: target.to_owned(),
                columns,
            })
        }
        _ => Err(format!("unknown key: <{name}>")),
    }
}

fn mouse(kind: MouseKind, target: &str) -> Gesture {
    Gesture::Mouse {
        kind,
        target: target.to_owned(),
    }
}

/// Where a `<...>` that starts the text ends. A step may hold `>` itself, as in
/// `<step add alert cyclomatic function > 1 warning>`, so it ends at a `>` that comes
/// last, or right before the next `<`.
fn closing(text: &str) -> Option<usize> {
    if !text.starts_with("<step ") {
        return text.find('>');
    }
    text.char_indices()
        .filter(|(_, character)| *character == '>')
        .map(|(index, _)| index)
        .find(|index| {
            let after = text.get(index + 1..).unwrap_or_default();
            after.is_empty() || after.starts_with('<')
        })
}

/// Reads keys written in Vim's notation, as the specification writes them: each
/// character is a key, and `<...>` names a key or a gesture.
///
/// # Errors
///
/// Fails on a `<` that is never closed, or a name that is not known.
pub fn parse_gestures(text: &str) -> Result<Vec<Gesture>, String> {
    let mut gestures = Vec::new();
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        if first == '<' {
            let end = closing(rest).ok_or_else(|| format!("unclosed < in {text}"))?;
            let name = rest.get(1..end).unwrap_or_default();
            gestures.push(named(name)?);
            rest = rest.get(end + 1..).unwrap_or_default();
        } else {
            gestures.push(Gesture::Key(Key::Char(first)));
            rest = rest.get(first.len_utf8()..).unwrap_or_default();
        }
    }
    Ok(gestures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_are_keys_and_names_are_in_angle_brackets() {
        assert_eq!(
            parse_gestures("g<Space>d<CR><C-o><lt>").unwrap(),
            vec![
                Gesture::Key(Key::Char('g')),
                Gesture::Key(Key::Char(' ')),
                Gesture::Key(Key::Char('d')),
                Gesture::Key(Key::Enter),
                Gesture::Key(Key::Ctrl('o')),
                Gesture::Key(Key::Char('<')),
            ]
        );
    }

    #[test]
    fn gestures_name_their_target_and_steps_keep_their_words() {
        assert_eq!(
            parse_gestures("<click symbol shop::catalog><wait 600><resize 80x24><step edit a.rs>")
                .unwrap(),
            vec![
                Gesture::Mouse {
                    kind: MouseKind::Click,
                    target: "symbol shop::catalog".to_owned(),
                },
                Gesture::Wait(600),
                Gesture::Resize {
                    columns: 80,
                    rows: 24,
                },
                Gesture::Step("edit a.rs".to_owned()),
            ]
        );
    }

    #[test]
    fn a_step_may_hold_a_greater_than_sign() {
        assert_eq!(
            parse_gestures("<step add alert cyclomatic function > 1 warning><Space>r").unwrap(),
            vec![
                Gesture::Step("add alert cyclomatic function > 1 warning".to_owned()),
                Gesture::Key(Key::Char(' ')),
                Gesture::Key(Key::Char('r')),
            ]
        );
    }

    #[test]
    fn an_unknown_name_is_refused() {
        assert!(parse_gestures("<nonsense>").is_err());
        assert!(parse_gestures("<Esc").is_err());
    }
}

//! The mascot: a one-eyed watcher that shows what the analysis is doing, drawn in
//! half-block cells from the pixels of `specs/tui/design/source/Mascot.dc.html`.

use crate::state::Pose;
use crate::tokens::Token;

/// How wide the mascot is, in cells.
pub const WIDTH: u16 = 16;
/// How tall the mascot is, in cells: two pixels to a cell.
pub const HEIGHT: u16 = 7;

/// How long a blink lasts, and how often one comes.
const BLINK_MS: u64 = 150;
const BLINK_EVERY_MS: u64 = 6000;
/// How long the pupil rests in each place while scanning.
const SCAN_MS: u64 = 400;

const TOP: [&str; 4] = [
    ".......HH.......",
    "......HHHH......",
    "....BBBBBBBB....",
    "..BBBBBBBBBBBB..",
];
const LOW: [&str; 4] = [
    ".BBBBBBBBBBBBBB.",
    ".BBBBMWMMWMBBBB.",
    "..BBBBBBBBBBBB..",
    "...DD......DD...",
];
const WATCHING: [&str; 6] = [
    ".BBBWWWWWWWWBBB.",
    ".BBWWWIIIIWWWBB.",
    "BBBWWIIPPIIWWBBB",
    "BBBWWIIPPIIWWBBB",
    ".BBWWWIIIIWWWBB.",
    ".BBBWWWWWWWWBBB.",
];
const SCANNING: [&str; 6] = [
    ".BBBWWWWWWWWBBB.",
    ".BBWIIIIWWWWWBB.",
    "BBBIIPPIIWWWWBBB",
    "BBBIIPPIIWWWWBBB",
    ".BBWIIIIWWWWWBB.",
    ".BBBWWWWWWWWBBB.",
];
const BLINK: [&str; 6] = [
    ".BBBBBBBBBBBBBB.",
    ".BBBBBBBBBBBBBB.",
    "BBBBBBBBBBBBBBBB",
    "BBBDDDDDDDDDDBBB",
    ".BBBBBBBBBBBBBB.",
    ".BBBBBBBBBBBBBB.",
];
const FOUND: [&str; 6] = [
    ".BBWWWWWWWWWWBB.",
    ".BBWWOOOOOOWWBB.",
    "BBBWOOPPPPOOWBBB",
    "BBBWOOPPPPOOWBBB",
    ".BBWWOOOOOOWWBB.",
    ".BBWWWWWWWWWWBB.",
];
const EMPTY: [&str; 6] = [
    ".BBBBBBBBBBBBBB.",
    ".BBLLLLLLLLLLBB.",
    "BBBWWIIPPIIWWBBB",
    "BBBWWIIPPIIWWBBB",
    ".BBWWWWWWWWWWBB.",
    ".BBBWWWWWWWWBBB.",
];

/// The pixels of the mascot in a pose, at a time in milliseconds, one row of letters for
/// each row of pixels. `.` is no pixel.
#[must_use]
pub fn pixels(pose: Pose, now: u64) -> Vec<String> {
    let eye: Vec<String> = match pose {
        Pose::Watching if now % BLINK_EVERY_MS < BLINK_MS && now > 0 => owned(&BLINK),
        Pose::Watching => owned(&WATCHING),
        Pose::Scanning => match (now / SCAN_MS) % 4 {
            0 => owned(&SCANNING),
            2 => SCANNING
                .iter()
                .map(|row| row.chars().rev().collect())
                .collect(),
            _ => owned(&WATCHING),
        },
        Pose::Found => owned(&FOUND),
        Pose::NothingHere => owned(&EMPTY),
    };
    owned(&TOP)
        .into_iter()
        .chain(eye)
        .chain(owned(&LOW))
        .collect()
}

fn owned(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| (*row).to_owned()).collect()
}

/// The token of a pixel letter.
#[must_use]
pub const fn token(letter: char) -> Option<Token> {
    match letter {
        'B' => Some(Token::PxB),
        'D' => Some(Token::PxD),
        'W' => Some(Token::PxW),
        'I' => Some(Token::PxI),
        'P' => Some(Token::PxP),
        'H' => Some(Token::PxH),
        'M' => Some(Token::PxM),
        'O' => Some(Token::PxO),
        'L' => Some(Token::PxL),
        _ => None,
    }
}

/// One cell of half-block art: its glyph, the token of its top pixel, or of its only
/// pixel, and the token of the bottom pixel when both are there and differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfBlock {
    pub glyph: char,
    pub token: Token,
    pub below: Option<Token>,
}

/// Turns rows of pixels into rows of cells, two pixels to a cell, as the design does:
/// `█` for two alike, `▀` for a top one, and `▄` for a bottom one alone.
#[must_use]
pub fn cells(pixels: &[String]) -> Vec<Vec<Option<HalfBlock>>> {
    pixels
        .chunks(2)
        .map(|pair| {
            let top: Vec<char> = pair
                .first()
                .map(|row| row.chars().collect())
                .unwrap_or_default();
            let bottom: Vec<char> = pair
                .get(1)
                .map(|row| row.chars().collect())
                .unwrap_or_default();
            (0..top.len().max(bottom.len()))
                .map(|index| {
                    let upper = top.get(index).copied().and_then(token);
                    let lower = bottom.get(index).copied().and_then(token);
                    match (upper, lower) {
                        (Some(upper), Some(lower)) if upper == lower => Some(HalfBlock {
                            glyph: '█',
                            token: upper,
                            below: None,
                        }),
                        (Some(upper), lower) => Some(HalfBlock {
                            glyph: '▀',
                            token: upper,
                            below: lower,
                        }),
                        (None, Some(lower)) => Some(HalfBlock {
                            glyph: '▄',
                            token: lower,
                            below: None,
                        }),
                        (None, None) => None,
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(pose: Pose, now: u64) -> Vec<String> {
        cells(&pixels(pose, now))
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.map_or(' ', |cell| cell.glyph))
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn the_watcher_is_drawn_as_the_design_draws_it() {
        assert_eq!(
            text(Pose::Watching, 0),
            vec![
                "      ▄██▄",
                "  ▄▄████████▄▄",
                " ██▀██▀▀▀▀██▀██",
                "████████████████",
                " ██▀██▀▀▀▀██▀██",
                " ████▀▀▀▀▀▀████",
                "  ▀▀▀▀▀▀▀▀▀▀▀▀",
            ]
        );
    }

    #[test]
    fn it_scans_from_side_to_side_and_blinks_now_and_then() {
        let left = text(Pose::Scanning, 0);
        let right = text(Pose::Scanning, SCAN_MS * 2);
        assert_ne!(left, right);
        assert_eq!(text(Pose::Scanning, SCAN_MS), text(Pose::Watching, 0));
        assert_ne!(
            text(Pose::Watching, BLINK_EVERY_MS),
            text(Pose::Watching, 0)
        );
        assert_eq!(
            text(Pose::Watching, BLINK_EVERY_MS + BLINK_MS),
            text(Pose::Watching, 0)
        );
        assert_eq!(text(Pose::Scanning, 0).len(), usize::from(HEIGHT));
    }
}

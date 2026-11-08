//! The style tokens of the design, generated from `specs/tui/design/tokens.json`.
//! Regenerate them when the design changes, never edit them by hand.

use ratatui::style::Color;

/// A style token of the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Fg,
    B,
    Dim,
    Faint,
    Acc,
    Accb,
    Key,
    H0,
    H1,
    H2,
    H3,
    On,
    Mode,
    Chip,
    Chipa,
    Chiph,
    Chipd,
    T0,
    T1,
    T2,
    T3,
    T4,
    Kw,
    Str,
    Pop,
    Popb,
    Popd,
    Popk,
    Pops,
    Poph,
    Line,
    Track,
    Sel,
    PxB,
    PxD,
    PxW,
    PxI,
    PxP,
    PxH,
    PxM,
    PxO,
    PxL,
}

/// What a token looks like in one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub underline: bool,
}

/// What a token looks like when a theme does not say.
const PLAIN: Look = Look {
    fg: None,
    bg: None,
    bold: false,
    underline: false,
};

/// The background of the screen in the dark theme.
pub const DARK_GROUND: Color = Color::Rgb(0x0F, 0x12, 0x16);

/// Each token in the dark theme, in the order of [`Token`].
const DARK: [Look; 42] = [
    Look {
        fg: Some(Color::Rgb(0xD5, 0xDA, 0xE1)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x94, 0xA3)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x76, 0x80, 0x8F)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: None,
        bold: true,
        underline: true,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x94, 0xA3)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xE3, 0xC8, 0x78)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xFF, 0x7A, 0x59)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0xD5, 0xDA, 0xE1)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xC9, 0xD0, 0xD9)),
        bg: Some(Color::Rgb(0x1C, 0x23, 0x2C)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: Some(Color::Rgb(0x1C, 0x23, 0x2C)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bg: Some(Color::Rgb(0x1C, 0x23, 0x2C)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x94, 0xA3)),
        bg: Some(Color::Rgb(0x1C, 0x23, 0x2C)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x94, 0xA3)),
        bg: Some(Color::Rgb(0x16, 0x1B, 0x22)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xB8, 0xC1, 0xCC)),
        bg: Some(Color::Rgb(0x23, 0x2B, 0x35)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF0, 0xE6, 0xC8)),
        bg: Some(Color::Rgb(0x4F, 0x44, 0x20)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xFF, 0xF0, 0xE0)),
        bg: Some(Color::Rgb(0xA5, 0x52, 0x1A)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bg: Some(Color::Rgb(0xB9, 0x3A, 0x1C)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8F, 0xB8, 0xDE)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xB4, 0xC4, 0xA4)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xD5, 0xDA, 0xE1)),
        bg: Some(Color::Rgb(0x17, 0x1C, 0x23)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bg: Some(Color::Rgb(0x17, 0x1C, 0x23)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x94, 0xA3)),
        bg: Some(Color::Rgb(0x17, 0x1C, 0x23)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: Some(Color::Rgb(0x17, 0x1C, 0x23)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bg: Some(Color::Rgb(0x1E, 0x3A, 0x4C)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bg: Some(Color::Rgb(0x17, 0x1C, 0x23)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x3A, 0x43, 0x50)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x23, 0x2A, 0x33)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: None,
        bg: Some(Color::Rgb(0x1E, 0x3A, 0x4C)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x5E, 0x7F, 0x94)),
        bg: Some(Color::Rgb(0x5E, 0x7F, 0x94)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x3F, 0x58, 0x68)),
        bg: Some(Color::Rgb(0x3F, 0x58, 0x68)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8F, 0xB8, 0xDE)),
        bg: Some(Color::Rgb(0x8F, 0xB8, 0xDE)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x4C, 0x68, 0x78)),
        bg: Some(Color::Rgb(0x4C, 0x68, 0x78)),
        bold: false,
        underline: false,
    },
];

/// How a token looks in the dark theme.
#[must_use]
pub fn dark(token: Token) -> Look {
    DARK.get(token as usize).copied().unwrap_or(PLAIN)
}

/// The background of the screen in the light theme.
pub const LIGHT_GROUND: Color = Color::Rgb(0xF6, 0xF7, 0xF9);

/// Each token in the light theme, in the order of [`Token`].
const LIGHT: [Look; 42] = [
    Look {
        fg: Some(Color::Rgb(0x2A, 0x31, 0x3A)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x59, 0x63, 0x6F)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6A, 0x74, 0x80)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bg: None,
        bold: true,
        underline: true,
    },
    Look {
        fg: Some(Color::Rgb(0x59, 0x63, 0x6F)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8A, 0x6A, 0x00)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xB4, 0x53, 0x09)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xC2, 0x30, 0x0F)),
        bg: None,
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF6, 0xF7, 0xF9)),
        bg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF6, 0xF7, 0xF9)),
        bg: Some(Color::Rgb(0x2A, 0x31, 0x3A)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x2F, 0x37, 0x42)),
        bg: Some(Color::Rgb(0xE4, 0xE8, 0xED)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bg: Some(Color::Rgb(0xE4, 0xE8, 0xED)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xA8, 0x4A, 0x06)),
        bg: Some(Color::Rgb(0xE4, 0xE8, 0xED)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x59, 0x63, 0x6F)),
        bg: Some(Color::Rgb(0xE4, 0xE8, 0xED)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x59, 0x63, 0x6F)),
        bg: Some(Color::Rgb(0xF0, 0xF2, 0xF5)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x3A, 0x44, 0x50)),
        bg: Some(Color::Rgb(0xDD, 0xE2, 0xE8)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x5A, 0x35, 0x00)),
        bg: Some(Color::Rgb(0xF5, 0xCF, 0x8E)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bg: Some(Color::Rgb(0xB3, 0x55, 0x09)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bg: Some(Color::Rgb(0xB9, 0x3A, 0x1C)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x2F, 0x6D, 0xA8)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x4F, 0x6B, 0x3A)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x2A, 0x31, 0x3A)),
        bg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x59, 0x63, 0x6F)),
        bg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0B, 0x6E, 0x99)),
        bg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0xCF, 0xE6, 0xF2)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xB4, 0x53, 0x09)),
        bg: Some(Color::Rgb(0xFF, 0xFF, 0xFF)),
        bold: true,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xC3, 0xCA, 0xD3)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xDD, 0xE2, 0xE8)),
        bg: None,
        bold: false,
        underline: false,
    },
    Look {
        fg: None,
        bg: Some(Color::Rgb(0xCF, 0xE6, 0xF2)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x5E, 0x7F, 0x94)),
        bg: Some(Color::Rgb(0x5E, 0x7F, 0x94)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x3F, 0x58, 0x68)),
        bg: Some(Color::Rgb(0x3F, 0x58, 0x68)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bg: Some(Color::Rgb(0xF1, 0xF4, 0xF8)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bg: Some(Color::Rgb(0x6C, 0xC4, 0xE8)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x8F, 0xB8, 0xDE)),
        bg: Some(Color::Rgb(0x8F, 0xB8, 0xDE)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bg: Some(Color::Rgb(0x0F, 0x12, 0x16)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bg: Some(Color::Rgb(0xF2, 0xA0, 0x3D)),
        bold: false,
        underline: false,
    },
    Look {
        fg: Some(Color::Rgb(0x4C, 0x68, 0x78)),
        bg: Some(Color::Rgb(0x4C, 0x68, 0x78)),
        bold: false,
        underline: false,
    },
];

/// How a token looks in the light theme.
#[must_use]
pub fn light(token: Token) -> Look {
    LIGHT.get(token as usize).copied().unwrap_or(PLAIN)
}

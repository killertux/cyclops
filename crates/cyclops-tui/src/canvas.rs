//! Drawing on a grid of cells with the design's tokens, and remembering where each thing
//! the mouse can point at was drawn.

use crate::mascot::HalfBlock;
use crate::state::{Target, Theme};
use crate::tokens::{self, Look, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

/// Tokens that mark where you are, which reverse video stands for without color.
const fn reversed_without_color(token: Token) -> bool {
    matches!(
        token,
        Token::Sel | Token::On | Token::Mode | Token::Pops | Token::Chipa
    )
}

/// The style of a token in a theme.
#[must_use]
pub fn style(theme: Theme, token: Token) -> Style {
    let look: Look = match theme {
        Theme::Dark | Theme::NoColor => tokens::dark(token),
        Theme::Light => tokens::light(token),
    };
    let mut style = Style::default();
    if theme != Theme::NoColor {
        if let Some(fg) = look.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = look.bg {
            style = style.bg(bg);
        }
    } else if reversed_without_color(token) {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if look.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if look.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

/// How many cells a text takes. Every glyph the design uses is one cell wide.
#[must_use]
pub fn width(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

/// A text cut to fit, ending in `…` when it does not.
#[must_use]
pub fn fit(text: &str, room: u16) -> String {
    if width(text) <= room {
        return text.to_owned();
    }
    let mut kept: String = text
        .chars()
        .take(usize::from(room.saturating_sub(1)))
        .collect();
    if room > 0 {
        kept.push('…');
    }
    kept
}

/// A buffer to draw on.
pub struct Canvas<'a> {
    buffer: &'a mut Buffer,
    pub theme: Theme,
    pub hits: Vec<(Rect, Target)>,
}

impl<'a> Canvas<'a> {
    pub fn new(buffer: &'a mut Buffer, theme: Theme) -> Self {
        Self {
            buffer,
            theme,
            hits: Vec::new(),
        }
    }

    #[must_use]
    pub fn area(&self) -> Rect {
        self.buffer.area
    }

    #[must_use]
    pub fn style(&self, token: Token) -> Style {
        style(self.theme, token)
    }

    /// Paints the whole screen with its background.
    pub fn ground(&mut self) {
        let area = self.buffer.area;
        let ground = match self.theme {
            Theme::Dark => Style::default()
                .bg(tokens::DARK_GROUND)
                .fg(tokens::dark(Token::Fg).fg.unwrap_or_default()),
            Theme::Light => Style::default()
                .bg(tokens::LIGHT_GROUND)
                .fg(tokens::light(Token::Fg).fg.unwrap_or_default()),
            Theme::NoColor => Style::default(),
        };
        self.buffer.set_style(area, ground);
    }

    /// Writes a text from a cell, cut at the right edge of `clip`. Returns where it ends.
    pub fn text(&mut self, x: u16, y: u16, text: &str, token: Token, clip: Rect) -> u16 {
        let right = clip.x.saturating_add(clip.width);
        if y < clip.y || y >= clip.y.saturating_add(clip.height) || x >= right {
            return x;
        }
        let area = self.buffer.area;
        if y >= area.y + area.height || x >= area.x + area.width {
            return x;
        }
        let room = right.min(area.x + area.width) - x;
        let shown = fit(text, room);
        let style = self.style(token);
        let (end, _) = self
            .buffer
            .set_stringn(x, y, &shown, usize::from(room), style);
        end
    }

    /// Writes a text that ends at `right`, exclusive.
    pub fn text_right(&mut self, right: u16, y: u16, text: &str, token: Token, clip: Rect) {
        let x = right.saturating_sub(width(text)).max(clip.x);
        self.text(x, y, text, token, clip);
    }

    /// Gives an area the style of a token, keeping its text.
    pub fn paint(&mut self, area: Rect, token: Token) {
        let area = area.intersection(self.buffer.area);
        let style = self.style(token);
        self.buffer.set_style(area, style);
    }

    /// Clears an area and gives it the style of a token.
    pub fn fill(&mut self, area: Rect, token: Token) {
        let area = area.intersection(self.buffer.area);
        let style = self.style(token);
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = self.buffer.cell_mut((x, y)) {
                    cell.reset();
                    cell.set_symbol(" ");
                    cell.set_style(style);
                }
            }
        }
    }

    /// Draws a rounded box with a title on its top edge, and returns what is inside.
    pub fn pane(&mut self, area: Rect, title: &str, active: bool) -> Rect {
        let (border, heading) = if active {
            (Token::Acc, Token::Accb)
        } else {
            (Token::Line, Token::Dim)
        };
        self.frame(area, border, ['╭', '╮', '╰', '╯', '─', '│']);
        if !title.is_empty() && area.width > 6 {
            let shown = fit(&format!(" {title} "), area.width.saturating_sub(4));
            self.text(area.x + 2, area.y, &shown, heading, area);
        }
        inner(area)
    }

    /// Draws the edges of a box with these glyphs: four corners, then the two lines.
    pub fn frame(&mut self, area: Rect, token: Token, glyphs: [char; 6]) {
        if area.width < 2 || area.height < 2 {
            return;
        }
        let [top_left, top_right, bottom_left, bottom_right, across, down] = glyphs;
        let right = area.x + area.width - 1;
        let bottom = area.y + area.height - 1;
        let across = across.to_string().repeat(usize::from(area.width - 2));
        self.text(
            area.x,
            area.y,
            &format!("{top_left}{across}{top_right}"),
            token,
            area,
        );
        self.text(
            area.x,
            bottom,
            &format!("{bottom_left}{across}{bottom_right}"),
            token,
            area,
        );
        for y in area.y + 1..bottom {
            self.text(area.x, y, &down.to_string(), token, area);
            self.text(right, y, &down.to_string(), token, area);
        }
    }

    /// Draws half-block art with its top left corner at a cell.
    pub fn art(&mut self, x: u16, y: u16, rows: &[Vec<Option<HalfBlock>>]) {
        let theme = self.theme;
        for (row_index, row) in rows.iter().enumerate() {
            for (column, cell) in row.iter().enumerate() {
                let Some(block) = cell else {
                    continue;
                };
                let at = (
                    x + u16::try_from(column).unwrap_or(0),
                    y + u16::try_from(row_index).unwrap_or(0),
                );
                let look = |token: Token| match theme {
                    Theme::Light => tokens::light(token),
                    Theme::Dark | Theme::NoColor => tokens::dark(token),
                };
                let mut cell_style = Style::default();
                if theme != Theme::NoColor {
                    if let Some(fg) = look(block.token).fg {
                        cell_style = cell_style.fg(fg);
                    }
                    if let Some(bg) = block.below.and_then(|below| look(below).fg) {
                        cell_style = cell_style.bg(bg);
                    }
                }
                if let Some(target) = self.buffer.cell_mut(at) {
                    target.set_char(block.glyph);
                    target.set_style(cell_style);
                }
            }
        }
    }

    /// Remembers that something the mouse can point at is here.
    pub fn hit(&mut self, area: Rect, target: Target) {
        self.hits.push((area, target));
    }
}

/// What is inside a box's border.
#[must_use]
pub const fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// The text of a buffer, one line for each row.
#[must_use]
pub fn screen_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut lines = Vec::new();
    for y in area.y..area.y + area.height {
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_does_not_fit_ends_in_an_ellipsis() {
        assert_eq!(fit("orders", 10), "orders");
        assert_eq!(fit("orders", 6), "orders");
        assert_eq!(fit("orders", 4), "ord…");
        assert_eq!(fit("orders", 0), "");
    }

    #[test]
    fn without_color_a_selection_is_reversed() {
        let selection = style(Theme::NoColor, Token::Sel);
        assert!(selection.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(selection.bg, None);
        assert!(style(Theme::Dark, Token::Sel).bg.is_some());
    }
}

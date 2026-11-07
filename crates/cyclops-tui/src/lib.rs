//! The terminal interface, as a model, an update and a view. It holds no analysis: it
//! asks the core for what it shows, and asks whoever runs it to analyze. See
//! `specs/tui/`.

pub mod app;
pub mod canvas;
pub mod draw;
pub mod input;
mod run;
pub mod state;
mod tokens;

pub use app::App;
pub use run::{Backend, run};

use cyclops_application::analysis::ProjectAnalysis;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use state::Effect;

/// What the interface needs from the core, through whoever composes it.
pub trait Core {
    /// Analyzes the project. `tests` overrides the test code setting for the session,
    /// and `precise` asks for the indexers.
    ///
    /// # Errors
    ///
    /// Says why the analysis was refused, such as settings that are wrong.
    fn analyze(&self, tests: Option<bool>, precise: bool) -> Result<ProjectAnalysis, String>;

    /// Saves default settings for the team.
    ///
    /// # Errors
    ///
    /// Says why they could not be saved.
    fn init(&self) -> Result<String, String>;
}

/// Does what the interface asked for straight away, until it asks for nothing more. Tests
/// and the specification drive the interface this way, so that it has always settled.
pub fn settle(app: &mut App, core: &dyn Core) {
    loop {
        let effects = app.take_effects();
        if effects.is_empty() {
            return;
        }
        for effect in effects {
            match effect {
                Effect::Analyze { tests, precise } => app.analyzed(core.analyze(tests, precise)),
                Effect::Init => app.initialized(core.init()),
                Effect::Quit => {}
            }
        }
    }
}

/// Draws the interface into a buffer of its own size.
pub fn render(app: &mut App) -> Buffer {
    let (columns, rows) = app.size();
    let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
    draw::draw(app, &mut buffer);
    buffer
}

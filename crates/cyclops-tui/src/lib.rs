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

use app::Heard;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::ports::{Partial, Progress, Step};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use state::Effect;
use std::sync::Mutex;

/// What the interface needs from the core, through whoever composes it.
pub trait Core {
    /// Analyzes the project, telling `progress` how it goes. `tests` overrides the test
    /// code setting for the session, and `precise` asks for the indexers.
    ///
    /// # Errors
    ///
    /// Says why the analysis was refused or stopped, such as settings that are wrong.
    fn analyze(
        &self,
        tests: Option<bool>,
        precise: bool,
        progress: &dyn Progress,
    ) -> Result<ProjectAnalysis, String>;

    /// Saves default settings for the team.
    ///
    /// # Errors
    ///
    /// Says why they could not be saved.
    fn init(&self) -> Result<String, String>;

    /// Whether precise analysis can run now.
    ///
    /// # Errors
    ///
    /// Says why not, such as a tool that is not installed.
    fn precise_ready(&self) -> Result<(), String>;

    /// Opens a file of the project in the editor, at a line, and returns once it closes.
    ///
    /// # Errors
    ///
    /// Says why the editor could not run.
    fn open(&self, path: &str, line: u32) -> Result<(), String>;

    /// Puts text on the clipboard.
    ///
    /// # Errors
    ///
    /// Says why the clipboard could not be used.
    fn copy(&self, text: &str) -> Result<(), String>;
}

/// Collects what an analysis says, to hand to the interface once it is done.
#[derive(Default)]
struct Recorded(Mutex<Vec<Heard>>);

impl Recorded {
    fn push(&self, heard: Heard) {
        if let Ok(mut all) = self.0.lock() {
            all.push(heard);
        }
    }
}

impl Progress for Recorded {
    fn step(&self, step: Step) -> bool {
        self.push(Heard::Step(step));
        true
    }

    fn files(&self, done: usize, of: usize) {
        self.push(Heard::Files(done, of));
    }

    fn partial(&self, partial: Partial) {
        self.push(Heard::Partial(partial));
    }

    fn indexing(&self, indexer: &str) {
        self.push(Heard::Indexer(indexer.to_owned()));
    }
}

/// Does one thing the interface asked for, straight away, other than analyzing.
pub fn perform(app: &mut App, core: &dyn Core, effect: Effect) {
    match effect {
        Effect::Analyze { tests, precise } => {
            let recorded = Recorded::default();
            let result = core.analyze(tests, precise, &recorded);
            for heard in recorded.0.into_inner().unwrap_or_default() {
                app.heard(heard);
            }
            app.analyzed(result);
        }
        Effect::Init => app.initialized(core.init()),
        Effect::CheckPrecise => app.precise_checked(core.precise_ready()),
        Effect::Open { path, line } => app.editor_closed(core.open(&path, line)),
        Effect::Copy(text) => {
            if let Err(reason) = core.copy(&text) {
                app.tell(reason);
            }
        }
        Effect::Quit => {}
    }
}

/// Does what the interface asked for straight away, until it asks for nothing more. Tests
/// drive the interface this way, so that it has always settled.
pub fn settle(app: &mut App, core: &dyn Core) {
    loop {
        let effects = app.take_effects();
        if effects.is_empty() {
            return;
        }
        for effect in effects {
            perform(app, core, effect);
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

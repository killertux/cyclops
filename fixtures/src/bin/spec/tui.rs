//! What the `Tui` fixture needs: the interface opened on a sample, driven by keys, with
//! time that only passes when a row says so.

use crate::scenario::{Scenario, no_cyclops};
use crate::scratch::Scratch;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::ports::FileSource;
use cyclops_cli::{init, interface_theme};
use cyclops_domain::path::RelativePath;
use cyclops_infra::ProjectFiles;
use cyclops_tui::app::space_menu;
use cyclops_tui::canvas::screen_text;
use cyclops_tui::input::{Gesture, Input, MouseKind, parse_gestures, parse_size};
use cyclops_tui::state::{Startup, Target, ThemeChoice, View};
use cyclops_tui::{App, Core, render, settle};
use std::collections::BTreeMap;
use std::time::SystemTime;

/// The size of the terminal when a row does not say.
const DEFAULT_SIZE: (u16, u16) = (120, 36);

/// The core, as a row's scenario offers it.
struct Offered<'a>(&'a Scenario);

impl Core for Offered<'_> {
    fn analyze(&self, tests: Option<bool>, precise: bool) -> Result<ProjectAnalysis, String> {
        self.0.analyze_for(tests, precise)
    }

    fn init(&self) -> Result<String, String> {
        init(self.0.root()).map_err(|error| error.to_string())
    }
}

/// The files of a project, with when each last changed.
fn stamps(scenario: &Scenario) -> BTreeMap<RelativePath, (u64, Option<SystemTime>)> {
    let Ok(files) = ProjectFiles::open(scenario.root()) else {
        return BTreeMap::new();
    };
    let Ok(found) = files.discover() else {
        return BTreeMap::new();
    };
    found
        .files
        .into_iter()
        .map(|path| {
            let metadata = std::fs::metadata(scenario.root().join(path.as_str())).ok();
            let size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
            let modified = metadata.and_then(|metadata| metadata.modified().ok());
            (path, (size, modified))
        })
        .collect()
}

/// An interface opened on a sample.
pub struct Session {
    pub scenario: Scenario,
    pub app: App,
    pub before: std::collections::BTreeSet<String>,
    clock: u64,
    seen: BTreeMap<RelativePath, (u64, Option<SystemTime>)>,
}

impl Session {
    /// Copies a sample, takes the steps, and opens the interface on it.
    pub fn open(workspace: &str, steps: &str, size: &str) -> Result<Self, String> {
        let scratch = Scratch::from_sample(workspace).map_err(|e| e.to_string())?;
        let mut scenario = Scenario::new(scratch);
        scenario.steps(steps, &no_cyclops)?;
        scenario.finish()?;
        let (columns, rows) = if size.trim().is_empty() {
            DEFAULT_SIZE
        } else {
            parse_size(size).ok_or_else(|| format!("not a size: {size}"))?
        };
        let environment = scenario.environment();
        let (theme, no_color) = interface_theme(scenario.root(), &environment);
        let background = scenario.background().and_then(ThemeChoice::parse);
        let mut app = App::new(Startup {
            columns,
            rows,
            theme,
            background,
            no_color,
        });
        settle(&mut app, &Offered(&scenario));
        let before = crate::cli::files_in(scenario.root());
        let seen = stamps(&scenario);
        Ok(Self {
            scenario,
            app,
            before,
            clock: 0,
            seen,
        })
    }

    fn settle(&mut self) {
        let analyzed = self.app.has_effects();
        settle(&mut self.app, &Offered(&self.scenario));
        if analyzed {
            self.seen = stamps(&self.scenario);
        }
    }

    /// Where the mouse has to be to point at a target, such as `symbol shop::orders`.
    fn position_of(&mut self, target: &str) -> Result<(u16, u16), String> {
        render(&mut self.app);
        let (word, rest) = target.split_once(' ').unwrap_or((target, ""));
        let wanted = match word {
            "symbol" => Target::Symbol(cyclops_domain::symbol::SymbolId::new(rest)),
            "tab" => Target::Tab(
                View::ALL
                    .into_iter()
                    .find(|view| view.name() == rest)
                    .ok_or_else(|| format!("no tab {rest}"))?,
            ),
            "menu" => {
                let key = space_menu()
                    .iter()
                    .flat_map(|(_, entries)| entries.iter())
                    .find(|(_, label)| *label == rest)
                    .map(|(key, _)| *key)
                    .ok_or_else(|| format!("no menu entry {rest}"))?;
                Target::Menu(key)
            }
            other => return Err(format!("unknown target: {other}")),
        };
        self.app
            .hits()
            .iter()
            .find(|(_, target)| *target == wanted)
            .map(|(area, _)| (area.x + area.width / 2, area.y + area.height / 2))
            .ok_or_else(|| format!("{target} is not on screen"))
    }

    /// Does what the `keys` column says.
    pub fn press(&mut self, keys: &str) -> Result<(), String> {
        for gesture in parse_gestures(keys)? {
            match gesture {
                Gesture::Key(key) => self.app.update(Input::Key(key)),
                Gesture::Mouse { kind, target } => {
                    let (column, row) = if matches!(
                        kind,
                        MouseKind::WheelUp
                            | MouseKind::WheelDown
                            | MouseKind::CtrlWheelUp
                            | MouseKind::CtrlWheelDown
                    ) && target.is_empty()
                    {
                        (0, 2)
                    } else {
                        self.position_of(&target)?
                    };
                    self.app.update(Input::Mouse { kind, column, row });
                }
                Gesture::Wait(milliseconds) => {
                    self.clock += milliseconds;
                    self.app.update(Input::Tick(self.clock));
                    if stamps(&self.scenario) != self.seen {
                        self.app.update(Input::FilesChanged);
                    }
                }
                Gesture::Resize { columns, rows } => {
                    self.app.update(Input::Resize { columns, rows });
                }
                Gesture::Step(step) => self.scenario.steps(&step, &no_cyclops)?,
                Gesture::Resume => {}
            }
            self.settle();
        }
        Ok(())
    }

    /// The text on screen.
    pub fn screen(&mut self) -> String {
        screen_text(&render(&mut self.app))
    }
}

//! What the `Tui` fixture needs: the interface opened on a sample, driven by keys, with
//! time that only passes when a row says so. Analyses run on a thread of their own, as
//! they do in a terminal, and may be held at a step so that a row can look mid-way.

use crate::scenario::{Scenario, no_cyclops};
use crate::scratch::Scratch;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::diagram::{DiagramView, Level};
use cyclops_application::ports::{FileSource, Partial, Progress, Step};
use cyclops_cli::{
    export_view, indexer_reports, init, interface_startup, precise_ready, save_rule, toggle_rule,
};
use cyclops_domain::metric::MetricId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::settings::RawValue;
use cyclops_infra::ProjectFiles;
use cyclops_tui::app::{Heard, space_menu};
use cyclops_tui::canvas::screen_text;
use cyclops_tui::input::{Gesture, Input, MouseKind, parse_gestures, parse_size};
use cyclops_tui::state::{Effect, IndexerReport, Startup, Target, ThemeChoice, View};
use cyclops_tui::{App, Core, perform, render};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

/// The size of the terminal when a row does not say.
const DEFAULT_SIZE: (u16, u16) = (120, 36);

/// The core, as a row's scenario offers it, for everything but analyzing.
struct Offered<'a>(&'a Scenario);

impl Core for Offered<'_> {
    fn analyze(
        &self,
        tests: Option<bool>,
        precise: bool,
        progress: &dyn Progress,
    ) -> Result<ProjectAnalysis, String> {
        self.0.analysis_job(tests, precise)(progress)
    }

    fn init(&self) -> Result<String, String> {
        init(self.0.root()).map_err(|error| error.to_string())
    }

    fn precise_ready(&self) -> Result<(), String> {
        precise_ready(self.0.root(), &self.0.environment())
    }

    fn open(&self, _path: &str, _line: u32) -> Result<(), String> {
        Ok(())
    }

    fn copy(&self, _text: &str) -> Result<(), String> {
        Ok(())
    }

    fn save_rule(
        &self,
        fields: &[(String, RawValue)],
        yours: bool,
        position: Option<usize>,
    ) -> Result<String, String> {
        save_rule(
            self.0.root(),
            &self.0.environment(),
            fields,
            yours,
            position,
        )
    }

    fn toggle_rule(&self, yours: bool, position: usize, enabled: bool) -> Result<(), String> {
        toggle_rule(
            self.0.root(),
            &self.0.environment(),
            yours,
            position,
            enabled,
        )
    }

    fn read(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.0.root().join(path)).map_err(|error| error.to_string())
    }

    fn indexers(&self) -> Vec<IndexerReport> {
        indexer_reports(self.0.root(), &self.0.environment())
    }

    fn export(
        &self,
        format: &str,
        path: &str,
        view: &DiagramView,
        level: Level,
        overlay: Option<MetricId>,
    ) -> Result<String, String> {
        export_view(self.0.root(), format, path, view, level, overlay)
    }
}

/// What an analysis thread says.
enum Event {
    Heard(Heard),
    /// It is waiting before the step a row named.
    Paused,
    Done(Box<Result<ProjectAnalysis, String>>),
}

/// Whether a held analysis goes on.
enum Control {
    Go,
    Stop,
}

/// Passes what an analysis says to the session, and holds it before one step.
struct Watched {
    events: Sender<Event>,
    pause: Option<Step>,
    /// What the indexer says about its progress, when a row makes it say something.
    percent: Option<u8>,
    control: Mutex<Receiver<Control>>,
    /// The session stopped listening, so the analysis should stop.
    closed: AtomicBool,
}

impl Watched {
    fn tell(&self, event: Event) {
        if self.events.send(event).is_err() {
            self.closed.store(true, Ordering::Relaxed);
        }
    }

    /// Waits for the session to let the analysis go on.
    fn hold(&self) -> bool {
        self.tell(Event::Paused);
        let Ok(control) = self.control.lock() else {
            return false;
        };
        matches!(control.recv(), Ok(Control::Go))
    }
}

impl Progress for Watched {
    fn step(&self, step: Step) -> bool {
        if self.pause == Some(step) && !self.hold() {
            return false;
        }

        self.tell(Event::Heard(Heard::Step(step)));
        !self.closed.load(Ordering::Relaxed)
    }

    fn files(&self, done: usize, of: usize) {
        self.tell(Event::Heard(Heard::Files(done, of)));
    }

    fn partial(&self, partial: Partial) {
        self.tell(Event::Heard(Heard::Partial(partial)));
    }

    fn indexing(&self, indexer: &str) {
        self.tell(Event::Heard(Heard::Indexer(indexer.to_owned())));
        if let Some(percent) = self.percent {
            self.tell(Event::Heard(Heard::Percent(percent)));
        }
    }
}

/// An analysis running on its thread. The thread ends with the answer it could not give,
/// if the session stopped listening.
struct Job {
    events: Receiver<Event>,
    control: Sender<Control>,
    thread: JoinHandle<Result<(), SendError<Event>>>,
    paused: bool,
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
    pub before: BTreeSet<String>,
    clock: u64,
    seen: BTreeMap<RelativePath, (u64, Option<SystemTime>)>,
    job: Option<Job>,
    /// The step the next analysis waits before, until one has.
    pause: Option<Step>,
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
        let pause = match scenario.pause() {
            Some(name) => Some(Step::parse(name).ok_or_else(|| format!("no step {name}"))?),
            None => None,
        };
        let background = scenario.background().and_then(ThemeChoice::parse);
        let app = App::new(Startup {
            columns,
            rows,
            background,
            ..interface_startup(scenario.root(), &scenario.environment())
        });
        let mut session = Self {
            before: BTreeSet::new(),
            seen: stamps(&scenario),
            scenario,
            app,
            clock: 0,
            job: None,
            pause,
        };
        session.settle()?;
        session.before = crate::cli::files_in(session.scenario.root());
        Ok(session)
    }

    fn start(&mut self, tests: Option<bool>, precise: bool) -> Result<(), String> {
        let (events, heard) = mpsc::channel();
        let (control, controlled) = mpsc::channel();
        let job = self.scenario.analysis_job(tests, precise);
        let watched = Watched {
            events,
            pause: self.pause,
            percent: self.scenario.indexer_percent(),
            control: Mutex::new(controlled),
            closed: AtomicBool::new(false),
        };
        let thread = thread::Builder::new()
            .spawn(move || {
                let result = job(&watched);
                watched.events.send(Event::Done(Box::new(result)))
            })
            .map_err(|error| error.to_string())?;
        self.job = Some(Job {
            events: heard,
            control,
            thread,
            paused: false,
        });
        Ok(())
    }

    /// Lets a held analysis carry on, or stops it.
    fn release(&mut self, control: Control) {
        let Some(job) = &mut self.job else {
            return;
        };
        if !job.paused {
            return;
        }
        job.paused = false;
        if job.control.send(control).is_err() {
            // The analysis already ended, so there is nothing left to wait for.
            self.job = None;
        }
    }

    /// Does what the interface asked for, and waits for analyses until they finish or
    /// are held.
    fn settle(&mut self) -> Result<(), String> {
        loop {
            for effect in self.app.take_effects() {
                match effect {
                    Effect::Analyze { tests, precise } => {
                        if self.job.is_none() {
                            self.start(tests, precise)?;
                        }
                    }
                    Effect::Quit => self.release(Control::Stop),
                    other => perform(&mut self.app, &Offered(&self.scenario), other),
                }
            }
            let Some(job) = &mut self.job else {
                if self.app.has_effects() {
                    continue;
                }
                return Ok(());
            };
            if job.paused {
                return Ok(());
            }
            match job.events.recv() {
                Ok(Event::Heard(heard)) => self.app.heard(heard),
                Ok(Event::Paused) => {
                    job.paused = true;
                    self.pause = None;
                }
                Ok(Event::Done(result)) => {
                    if let Some(job) = self.job.take() {
                        job.thread
                            .join()
                            .map_err(|_| "the analysis panicked")?
                            .map_err(|_| "nobody heard the analysis end")?;
                    }
                    if let Ok(analysis) = result.as_ref() {
                        self.scenario.note_indexers(analysis);
                    }
                    self.app.analyzed(*result);
                    self.seen = stamps(&self.scenario);
                }
                Err(_) => {
                    self.job = None;
                    return Err("the analysis ended without an answer".to_owned());
                }
            }
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
            "menu" => space_menu()
                .iter()
                .flat_map(|(_, entries)| entries.iter())
                .find(|(_, label)| *label == rest)
                .map_or_else(
                    || Target::Entry(rest.to_owned()),
                    |(key, _)| Target::Menu(*key),
                ),
            "breadcrumb" => Target::Breadcrumb(cyclops_domain::symbol::SymbolId::new(rest)),
            "chip" => match rest {
                "metric" => Target::Chip("metric"),
                "filter" => Target::Chip("filter"),
                "tests" => Target::Chip("tests"),
                "alerts" => Target::Chip("alerts"),
                "problems" => Target::Chip("problems"),
                other => return Err(format!("no chip {other}")),
            },
            "divider" => Target::Divider,
            "more" => {
                return self
                    .app
                    .hits()
                    .iter()
                    .find(|(_, target)| matches!(target, Target::More(_)))
                    .map(|(area, _)| (area.x + area.width / 2, area.y + area.height / 2))
                    .ok_or_else(|| "there is no more card".to_owned());
            }
            "column" => Target::Column(
                cyclops_domain::metric::MetricId::parse(rest)
                    .ok_or_else(|| format!("no column {rest}"))?,
            ),
            "metric" => Target::Metric(
                cyclops_domain::metric::MetricId::parse(&rest.replace(' ', "_"))
                    .ok_or_else(|| format!("no metric {rest}"))?,
            ),
            "rule" => Target::Rule(
                self.app
                    .listed_rules()
                    .iter()
                    .position(|(_, rule)| rule.title() == rest)
                    .ok_or_else(|| format!("no rule {rest}"))?,
            ),
            "problem" => Target::Problem(
                self.app
                    .analysis()
                    .and_then(|analysis| {
                        analysis.problems().iter().position(|problem| {
                            problem.path().is_some_and(|path| path.as_str() == rest)
                        })
                    })
                    .ok_or_else(|| format!("no problem {rest}"))?,
            ),
            "section" => Target::Section(
                cyclops_tui::state::Section::ALL
                    .into_iter()
                    .find(|section| section.name() == rest)
                    .ok_or_else(|| format!("no section {rest}"))?,
            ),
            "line" => Target::Line(rest.parse().map_err(|_| format!("not a line: {rest}"))?),
            "popup" => Target::Popup,
            "outside" => return self.outside_popup(),
            other => return Err(format!("unknown target: {other}")),
        };
        self.app
            .hits()
            .iter()
            .find(|(_, target)| *target == wanted)
            .map(|(area, _)| (area.x + area.width / 2, area.y + area.height / 2))
            .ok_or_else(|| format!("{target} is not on screen"))
    }

    /// A place on the screen outside the open popup: a bottom corner it does not cover.
    fn outside_popup(&self) -> Result<(u16, u16), String> {
        let popup = self
            .app
            .hits()
            .iter()
            .find(|(_, target)| *target == Target::Popup)
            .map(|(area, _)| *area)
            .ok_or_else(|| "there is no popup".to_owned())?;
        let (columns, rows) = self.app.size();
        let bottom = rows.saturating_sub(1);
        [(0, bottom), (columns.saturating_sub(1), bottom), (0, 0)]
            .into_iter()
            .find(|(x, y)| {
                let inside_x = *x >= popup.x && *x < popup.x + popup.width;
                let inside_y = *y >= popup.y && *y < popup.y + popup.height;
                !(inside_x && inside_y)
            })
            .ok_or_else(|| "the popup covers the screen".to_owned())
    }

    /// Does what the `keys` column says.
    pub fn press(&mut self, keys: &str) -> Result<(), String> {
        for gesture in parse_gestures(keys)? {
            match gesture {
                Gesture::Key(key) => self.app.update(Input::Key(key)),
                Gesture::Mouse { kind, target } => {
                    let wheel = matches!(
                        kind,
                        MouseKind::WheelUp
                            | MouseKind::WheelDown
                            | MouseKind::CtrlWheelUp
                            | MouseKind::CtrlWheelDown
                    );
                    let (column, row) = if wheel && target.is_empty() {
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
                Gesture::Resume => self.release(Control::Go),
                Gesture::Drag { target, columns } => {
                    let (column, row) = self.position_of(&target)?;
                    self.app.update(Input::Mouse {
                        kind: MouseKind::Click,
                        column,
                        row,
                    });
                    let to = column.saturating_add_signed(columns);
                    self.app.update(Input::Mouse {
                        kind: MouseKind::Drag,
                        column: to,
                        row,
                    });
                    self.app.update(Input::Mouse {
                        kind: MouseKind::Release,
                        column: to,
                        row,
                    });
                }
            }
            self.settle()?;
        }
        Ok(())
    }

    /// The text on screen.
    pub fn screen(&mut self) -> String {
        screen_text(&render(&mut self.app))
    }
}

impl Drop for Session {
    /// A row may end with an analysis still held. It is told to stop, and its thread ends
    /// on its own, since nobody listens any more.
    fn drop(&mut self) {
        self.release(Control::Stop);
    }
}

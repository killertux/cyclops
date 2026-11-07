//! Running the interface in a real terminal: the event loop, analysis on a worker thread
//! so that the interface never blocks, and the terminal's own background.

use crate::app::App;
use crate::input::{Input, Key, MouseKind};
use crate::state::{Effect, Startup, ThemeChoice};
use crate::{Core, draw};
use cyclops_application::analysis::ProjectAnalysis;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// What the core needs to run the interface: the analysis, and a way to learn that files
/// changed.
pub trait Backend: Core + Send + Sync {
    /// Calls `changed` whenever files of the project change, for as long as the returned
    /// value is kept. `changed` says whether anyone still wants to know.
    ///
    /// # Errors
    ///
    /// Says why the project cannot be watched.
    fn watch(&self, changed: Box<dyn Fn() -> bool + Send>) -> Result<Box<dyn Send>, String>;
}

/// What reaches the loop.
enum Message {
    Input(Input),
    Analyzed(Box<Result<ProjectAnalysis, String>>),
    Initialized(Result<String, String>),
}

/// Two clicks this close together are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

fn key_of(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    let key = match code {
        KeyCode::Char(character) if modifiers.contains(KeyModifiers::CONTROL) => {
            Key::Ctrl(character.to_ascii_lowercase())
        }
        KeyCode::Char(character) => Key::Char(character),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        _ => return None,
    };
    Some(key)
}

/// Reads the terminal's events, and the passing of time, into the loop.
fn read_events(sender: &Sender<Message>, started: Instant) {
    let mut last_click: Option<(Instant, u16, u16)> = None;
    loop {
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if sender.send(Message::Input(Input::Tick(elapsed))).is_err() {
            return;
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(_) => return,
        }
        let Ok(event) = event::read() else {
            return;
        };
        let input = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                key_of(key.code, key.modifiers).map(Input::Key)
            }
            Event::Mouse(mouse) => {
                let kind = match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let now = Instant::now();
                        let double = last_click.is_some_and(|(when, column, row)| {
                            now.duration_since(when) < DOUBLE_CLICK
                                && column == mouse.column
                                && row == mouse.row
                        });
                        last_click = Some((now, mouse.column, mouse.row));
                        Some(if double {
                            MouseKind::DoubleClick
                        } else {
                            MouseKind::Click
                        })
                    }
                    MouseEventKind::Down(MouseButton::Right) => Some(MouseKind::RightClick),
                    MouseEventKind::Moved => Some(MouseKind::Hover),
                    MouseEventKind::ScrollUp if mouse.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(MouseKind::CtrlWheelUp)
                    }
                    MouseEventKind::ScrollDown
                        if mouse.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        Some(MouseKind::CtrlWheelDown)
                    }
                    MouseEventKind::ScrollUp => Some(MouseKind::WheelUp),
                    MouseEventKind::ScrollDown => Some(MouseKind::WheelDown),
                    _ => None,
                };
                kind.map(|kind| Input::Mouse {
                    kind,
                    column: mouse.column,
                    row: mouse.row,
                })
            }
            Event::Resize(columns, rows) => Some(Input::Resize { columns, rows }),
            _ => None,
        };
        if let Some(input) = input
            && sender.send(Message::Input(input)).is_err()
        {
            return;
        }
    }
}

/// What the terminal says its background is, asked before the interface takes it over.
fn background() -> Option<ThemeChoice> {
    use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};
    match theme_mode(QueryOptions::default()) {
        Ok(ThemeMode::Light) => Some(ThemeChoice::Light),
        Ok(ThemeMode::Dark) => Some(ThemeChoice::Dark),
        Err(_) => None,
    }
}

/// Does what the interface asked for, on threads of its own.
fn perform(effects: Vec<Effect>, backend: &Arc<dyn Backend>, sender: &Sender<Message>) -> bool {
    let mut quit = false;
    for effect in effects {
        let backend = Arc::clone(backend);
        let sender = sender.clone();
        match effect {
            Effect::Analyze { tests, precise } => {
                // Sending fails only once the interface has closed, and then nobody
                // needs the answer, so the thread just ends with it.
                thread::spawn(move || {
                    let result = backend.analyze(tests, precise);
                    sender.send(Message::Analyzed(Box::new(result)))
                });
            }
            Effect::Init => {
                thread::spawn(move || {
                    let result = backend.init();
                    sender.send(Message::Initialized(result))
                });
            }
            Effect::Quit => quit = true,
        }
    }
    quit
}

fn handle(app: &mut App, message: Message) {
    match message {
        Message::Input(input) => app.update(input),
        Message::Analyzed(result) => app.analyzed(*result),
        Message::Initialized(result) => app.initialized(result),
    }
}

/// Runs the interface until the person quits.
///
/// # Errors
///
/// Fails if the terminal cannot be used.
pub fn run(backend: &Arc<dyn Backend>, theme: ThemeChoice, no_color: bool) -> io::Result<()> {
    let background = background();
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let size = terminal.size()?;
    let mut app = App::new(Startup {
        columns: size.width,
        rows: size.height,
        theme,
        background,
        no_color,
    });
    let (sender, receiver): (Sender<Message>, Receiver<Message>) = mpsc::channel();
    let started = Instant::now();
    let events = sender.clone();
    thread::spawn(move || read_events(&events, started));
    let changes = sender.clone();
    let watching = backend.watch(Box::new(move || {
        changes.send(Message::Input(Input::FilesChanged)).is_ok()
    }));
    let outcome = (|| -> io::Result<()> {
        loop {
            if perform(app.take_effects(), backend, &sender) || !app.running() {
                return Ok(());
            }
            terminal.draw(|frame| draw::draw(&mut app, frame.buffer_mut()))?;
            let Ok(message) = receiver.recv() else {
                return Ok(());
            };
            handle(&mut app, message);
            while let Ok(message) = receiver.try_recv() {
                handle(&mut app, message);
            }
        }
    })();
    drop(watching);
    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    outcome
}

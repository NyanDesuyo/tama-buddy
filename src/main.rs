mod api;
mod backend;
mod client;
mod pet;
mod store;
mod ui;
mod web;

use anyhow::{Context, Result, bail};
use client::BackendClient;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use pet::{Action, Pet};
use std::{
    io::IsTerminal,
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct App {
    pet: Option<Pet>,
    name: String,
    naming: bool,
    selected: usize,
    message: String,
    events: Vec<String>,
    frame: u64,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()) {
        PathBuf::from(path).join("tama-buddy/pet.sqlite3")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/tama-buddy/pet.sqlite3")
    } else {
        PathBuf::from("tama-buddy.sqlite3")
    }
}
fn main() -> Result<()> {
    let mut path = default_path();
    let mut status = false;
    let mut server = false;
    let mut url = "http://127.0.0.1:3000".to_owned();
    let mut bind: SocketAddr = "127.0.0.1:3000".parse()?;
    let mut custom_db = false;
    let mut custom_url = false;
    let mut custom_bind = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                path = PathBuf::from(args.next().context("--db requires a path")?);
                custom_db = true;
            }
            "--status" => status = true,
            "--server" => server = true,
            "--url" => {
                url = args.next().context("--url requires a URL")?;
                custom_url = true;
            }
            "--bind" => {
                bind = args
                    .next()
                    .context("--bind requires an IP:PORT")?
                    .parse()
                    .context("Invalid bind address; use IP:PORT")?;
                custom_bind = true;
            }
            "--help" | "-h" => {
                println!(
                    "Tama Buddy — a little friend in your terminal

Start backend: tama-buddy --server [--db PATH] [--bind IP:PORT]
Open web UI:   http://127.0.0.1:3000 in your browser
Open TUI:      tama-buddy [--url http://127.0.0.1:3000]
Check status:  tama-buddy --status [--url http://127.0.0.1:3000]

The backend owns SQLite and advances the simulation while the TUI is closed.
Default bind: 127.0.0.1:3000
Default save: {}

Controls: F feed · P play · C clean · S sleep/wake · M medicine
          ←/→ or H/L select · Enter act · R new egg (after death)
          Q / Esc / Ctrl+C quit",
                    path.display()
                );
                return Ok(());
            }
            _ => bail!("Unknown argument: {arg}. Use --help for usage."),
        }
    }
    if server {
        if status || custom_url {
            bail!("--server cannot be combined with --status or --url");
        }
        return backend::run(&path, bind);
    }
    if custom_db || custom_bind {
        bail!("--db and --bind are backend options. Use --server, or --url to connect a TUI.");
    }
    if !status && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()) {
        bail!("The TUI needs an interactive terminal. Use --status for text output.");
    }
    let client = BackendClient::new(&url)?;
    let snapshot = client.refresh()?;
    let pet = snapshot.pet;
    let message = snapshot.message.unwrap_or_else(|| {
        if pet.is_none() {
            "Connected. Name your buddy and press Enter to hatch.".into()
        } else {
            "Connected. Your little buddy missed you.".into()
        }
    });
    if status {
        if let Some(p) = pet {
            println!(
                "{} · {} · {}\nFullness {} | Joy {} | Energy {} | Hygiene {} | Health {}",
                p.name,
                p.stage(now()),
                p.mood(),
                p.fullness,
                p.happiness,
                p.energy,
                p.hygiene,
                p.health
            );
            println!(
                "Bond level {} | Care points {}",
                p.bond_level(),
                p.care_points
            );
        } else {
            println!("No buddy yet. Run cargo run to hatch your first egg.");
        }
        println!("Backend: {url}");
        return Ok(());
    }
    let mut app = App {
        naming: pet.is_none(),
        pet,
        name: String::new(),
        selected: 0,
        message,
        events: snapshot.events,
        frame: 0,
    };
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &client);
    ratatui::restore();
    result.context("Tama Buddy stopped unexpectedly")?;
    println!("See you soon! Your buddy lives on in the backend.");
    Ok(())
}
fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    client: &BackendClient,
) -> Result<()> {
    let mut last_poll = Instant::now();
    let mut disconnected = false;
    loop {
        let time = now();
        if last_poll.elapsed() >= Duration::from_secs(1) {
            match client.refresh() {
                Ok(snapshot) => {
                    if disconnected {
                        app.message = "Reconnected to your buddy.".into();
                    }
                    disconnected = false;
                    app.apply(snapshot);
                }
                Err(error) => {
                    disconnected = true;
                    app.message = format!("{error} Reconnecting…");
                }
            }
            last_poll = Instant::now();
        }
        terminal.draw(|f| ui::draw(f, app, time))?;
        app.frame = app.frame.wrapping_add(1);
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            || (!app.naming && key.code == KeyCode::Char('q'))
        {
            return Ok(());
        }
        if app.naming {
            match key.code {
                KeyCode::Char(c) if !c.is_control() && app.name.chars().count() < 20 => {
                    app.name.push(c)
                }
                KeyCode::Backspace => {
                    app.name.pop();
                }
                KeyCode::Enter => {
                    let name = app.name.trim();
                    let name = if name.is_empty() { "Mochi" } else { name }.to_owned();
                    match client.hatch(name) {
                        Ok(snapshot) => {
                            app.apply(snapshot);
                            app.naming = false;
                        }
                        Err(error) => app.message = error.to_string(),
                    }
                }
                _ => {}
            }
            continue;
        }
        let action = match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                app.selected = (app.selected + 4) % 5;
                None
            }
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
                app.selected = (app.selected + 1) % 5;
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => Some(
                [
                    Action::Feed,
                    Action::Play,
                    Action::Clean,
                    Action::Sleep,
                    Action::Medicine,
                ][app.selected],
            ),
            KeyCode::Char('f') => Some(Action::Feed),
            KeyCode::Char('p') => Some(Action::Play),
            KeyCode::Char('c') => Some(Action::Clean),
            KeyCode::Char('s') => Some(Action::Sleep),
            KeyCode::Char('m') => Some(Action::Medicine),
            KeyCode::Char('r') if app.pet.as_ref().is_some_and(|p| !p.alive()) => {
                app.naming = true;
                app.name.clear();
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            match client.care(action) {
                Ok(snapshot) => app.apply(snapshot),
                Err(error) => app.message = error.to_string(),
            }
        }
    }
}

impl App {
    fn apply(&mut self, snapshot: api::Snapshot) {
        if snapshot.pet.as_ref().is_some_and(Pet::alive) {
            self.naming = false;
        }
        self.pet = snapshot.pet;
        if self.events != snapshot.events
            && let Some(event) = snapshot.events.first()
        {
            self.message = event.clone();
        }
        self.events = snapshot.events;
        if let Some(message) = snapshot.message {
            self.message = message;
        }
    }
}

# Tama Buddy

A small Tamagotchi with a Rust HTTP backend, SQLite persistence, a Ratatui
terminal client, and a lightweight web UI. Hatch an egg, name your buddy, and
keep them fed, happy, rested, and clean. The backend keeps the simulation running
while your browser and TUI are closed.

## Run

Install a current stable Rust toolchain and a C compiler (for bundled SQLite), then:

Start the backend in one terminal:

```sh
cargo run --release -- --server
```

Open **http://127.0.0.1:3000** in your browser. The web UI is served by the same
backend: no Node.js, npm install, frontend build, or separate web server is
needed. Its HTML, CSS, JavaScript, and SVG illustrations are embedded in the
Rust binary, with no external fonts, scripts, or image downloads.

You can also open the TUI in another terminal:

```sh
cargo run --release
```

The backend listens on `http://127.0.0.1:3000` by default. It owns the SQLite
save and advances the simulation once per ten real minutes, checking every second.
The TUI reads state over HTTP once per second and sends care actions to the
backend; it never opens SQLite. Multiple terminal clients can connect safely.
The browser polls every two seconds while its tab is visible. Web and terminal
clients share the same pet, care cooldowns, and saved memories. The web UI
supports hatching, all five care actions, sleep/wake, and a new egg after death.
It reconnects automatically if the backend goes away temporarily.
Closing a client leaves the backend running. Stop the backend with Ctrl+C or
SIGTERM; all accepted actions are already committed.

The backend currently manages **one shared pet**, with no accounts or
authentication. Keep it on localhost for personal use. A custom `--bind` address
is available for development on a trusted network; access control and TLS would
be needed before deploying it publicly.

Use a terminal at least **60 columns × 30 rows**. Enter a name on the nursery
screen, or press Enter to call your buddy Mochi.

| Key | Action |
| --- | --- |
| F | Feed: restores fullness, makes a little mess |
| P | Play: boosts happiness, uses energy and food |
| C | Clean: restores hygiene |
| S | Toggle sleep: recover energy while sleeping |
| M | Medicine: restores health, lowers happiness a little |
| Left / Right, H / L, Tab | Select a care action |
| Enter / Space | Use selected action |
| R | Hatch a new buddy after the previous buddy dies |
| Q / Esc / Ctrl+C | Save and quit (while naming, use Esc / Ctrl+C) |

Care actions have a five-second cooldown; sleep/wake is immediate. Wake your
buddy before feeding, playing, bathing, or giving medicine.

## A little life

Stats change once per **ten real minutes** while the backend runs, even without
a client. If the backend stops, elapsed time is caught up when it restarts. Each
ten-minute step uses these rules:

- Fullness falls by two while awake and one while asleep.
- Happiness and hygiene fall by one.
- Energy falls by one while awake and recovers six while asleep.
- Each depleted fullness, hygiene, or energy stat costs three health.
- A well-fed, clean buddy recovers one health when no stat causes damage.

This gives you a few hours between care visits. A newly hatched buddy with its
starting stats can sleep for eight hours with full health; a hungry or sick
buddy still needs attention before bedtime. Neglect can still be fatal.

Growth follows real age: baby for the first hour, child until six hours, teen
until 24 hours, then adult. A long absence simulates at most twelve hours on
return. Neglect can be fatal; medicine cannot revive a buddy. Press R after death
to name and hatch a new one in the TUI, or choose “Welcome a new buddy” on the
web. The old pet is replaced only when you submit the new name. Age and growth
stop at death. The web UI shows progress to the next stage, and the journal
records growth milestones.

Helpful care also earns **bond points**: feeding, cleaning, and medicine award
four points each; playing awards six. Every 40 points raises the bond level.
Actions blocked by sleep, cooldown, or already-full stats earn no points.
Toggling sleep earns no points. The web UI tracks three achievements: first
helpful care, bond level two, and adulthood. A new egg begins its own bond and
achievement journey.

## SQLite saves

The backend commits state after every care action and simulation update.
The latest 50 journal messages are kept in SQLite; the dashboard shows the newest
few. No separate database server or SQLite installation is needed.

Default save location:

- `$XDG_DATA_HOME/tama-buddy/pet.sqlite3` when XDG_DATA_HOME is set
- Otherwise `$HOME/.local/share/tama-buddy/pet.sqlite3`
- Without either environment variable, `./tama-buddy.sqlite3`

Existing SQLite saves are migrated automatically, preserving your pet and
memories. Existing pets begin with zero bond points; historical care is not
retroactively scored. Choose the save on the backend, then
connect clients to its address:

```sh
cargo run -- --server --db ./my-buddy.sqlite3 --bind 127.0.0.1:4000
# In another terminal:
cargo run -- --url http://127.0.0.1:4000
cargo run -- --status --url http://127.0.0.1:4000
cargo run -- --help
```

`--db` and `--bind` apply to `--server`; `--url` selects a backend for the TUI or
`--status`. `--status` requires a running backend. The TUI reports connection
failures and polls again while open; care requests are not automatically retried.

## HTTP API

| Method | Path | Request | Result |
| --- | --- | --- | --- |
| GET | `/health` | None | JSON `"ok"` |
| GET | `/api/pet` | None | Current pet, recent events, and server time |
| POST | `/api/pet` | `{"name":"Mochi"}` | Hatch first pet or replace a dead pet |
| POST | `/api/pet/actions` | `{"action":"feed"}` | Apply a care action |

Supported actions: `feed`, `play`, `clean`, `sleep` (toggle), `medicine`.
Names are trimmed, limited to 20 characters, and must have no control characters.
A blank name becomes Mochi. JSON bodies are limited to 4 KiB.

Pet responses have the same shape:

```json
{
  "pet": {
    "name": "Mochi",
    "born": 1800000000,
    "last_tick": 1800000000,
    "fullness": 80,
    "happiness": 80,
    "energy": 80,
    "hygiene": 80,
    "health": 100,
    "sleeping": false,
    "last_action": 0,
    "care_points": 0,
    "died_at": null
  },
  "events": ["Mochi hatched! A tiny friendship begins."],
  "message": "Mochi hatched! A tiny friendship begins.",
  "server_time": 1800000000
}
```

Responses also include `progression` (or `null` without a pet): `stage`,
`next_stage`, `seconds_until_growth`, `growth_percent`, `bond_level`,
`bond_progress` (out of 40), `care_points`, and an `achievements` array of
`title`, `description`, and `unlocked` fields. `seconds_until_growth` is `null`
for adults and pets that have died. `died_at` records the simulated time of death.

`pet` is `null` before the first hatch; `message` may be `null`. Times are Unix
seconds. `events` contains the newest five messages, newest first.

Hatching returns HTTP 201, or 409 if a living pet already exists. Care before
hatching returns 404. Invalid JSON or names return 400; invalid action values or
missing fields return 422. Errors use `{"error":"description"}`. A valid care
request returns 200 with an explanatory message even when gameplay prevents the
action (for example, a cooldown or sleeping pet); inspect `message` and `pet`.
Cooldowns and all gameplay decisions are enforced by the backend.

```sh
curl http://127.0.0.1:3000/api/pet
curl -X POST http://127.0.0.1:3000/api/pet \
  -H 'Content-Type: application/json' -d '{"name":"Mochi"}'
curl -X POST http://127.0.0.1:3000/api/pet/actions \
  -H 'Content-Type: application/json' -d '{"action":"feed"}'
```

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build
python3 -m unittest discover -s tests -v
# Optional: Node.js is only needed for these frontend behavior tests.
node --test tests/web.test.cjs
```

The code separates simulation (`src/pet.rs`), SQLite storage (`src/store.rs`),
Axum backend (`src/backend.rs`), shared API types (`src/api.rs`), HTTP client
(`src/client.rs`), rendering (`src/ui.rs`), and CLI/input (`src/main.rs`).
Database work runs on blocking workers with serialized updates and SQLite
transactions that read the latest state before changing it. The integration
tests cover HTTP validation, concurrent clients, simulation without a TUI,
SQLite persistence across backend restarts, and the real terminal client.

The buildless browser UI lives in `web/` and is embedded through `src/web.rs`.
After changing its assets, rebuild/restart the backend to see the changes.
Frontend tests use Node's built-in test runner with a small DOM fixture; they
cover form validation, care controls, disconnects, and stale response handling.

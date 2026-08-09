[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)
![crates.io](https://img.shields.io/crates/v/portside-tui)

---

# portside

> [!NOTE]
>
> A `port` is where you dock and take stock before heading back out — `portside` is where you dock your work.

`portside` is a terminal-based work-session tracker. Start a session with a topic, pause and resume it, take timed breaks, and review how you've spent your time with a daily/weekly/cumulative history chart — all without leaving the terminal.

### Features

- Track work sessions by topic, with pause/resume and break states
- Timed or open-ended breaks, with a terminal bell + desktop notification when a timed break expires
- Resume any previous session, even one started days ago — its newly-accrued time is correctly split across days
- Drawer view of recent sessions for quick lookup and resuming
- History pane with daily, weekly, and cumulative views of time worked
- Neovim-style `:command` line alongside single-key normal-mode shortcuts
- Persistent local storage via SQLite — nothing leaves your machine

### Installation

#### Using Cargo

```bash
cargo install portside-tui
```

#### From Source

1. Clone the repository:

```bash
git clone https://github.com/jayanaxhf/portside.git
```

2. Navigate to the project directory:

```bash
cd portside
```

3. Build the project:

```bash
cargo install --path .
```

### Usage

Run the app with:

```bash
portside
```

Session data is stored in a SQLite database under your platform's standard data directory (via the `directories` crate), so it persists across runs.

#### Normal mode

| Key       | Action                        |
| --------- | ------------------------------ |
| `Space`   | Pause / resume active session  |
| `p`       | Pause                          |
| `r`       | Resume                         |
| `b`       | Toggle break                   |
| `c`       | Complete session                |
| `Tab` / `s` | Open the session list drawer |
| `:`       | Enter command mode              |
| `q`       | Quit                             |

#### Commands

Enter command mode with `:` and type one of the following:

| Command                     | Description                                             |
| ---------------------------- | -------------------------------------------------------- |
| `topic <name>`                | Start a new session under the given topic                |
| `pause`                        | Pause the active session                                  |
| `resume` / `play`              | Resume the active session                                 |
| `break [duration]`             | Toggle a break; optional duration like `5m` or `30s`      |
| `resume-previous [id]`         | Resume the most recent resumable session, or a specific one by id |
| `sessions`                     | Open the session list drawer                              |
| `history` / `hist [view]`      | Open the history pane (`daily`, `weekly`, or `cumulative`) |
| `complete` / `end` / `done`    | Mark the active session as complete                        |
| `quit` / `q`                   | Quit the application                                       |

#### Session list

| Key           | Action           |
| -------------- | ---------------- |
| `j` / `k`       | Select session    |
| `Enter`         | Resume selected   |
| `Esc` / `Tab`   | Close drawer      |

#### History pane

| Key           | Action                |
| -------------- | ---------------------- |
| `d`             | Daily view              |
| `w`             | Weekly view             |
| `c`             | Cumulative view         |
| `Esc` / `Tab`   | Close pane              |

### Contributing

Contributions to `portside` are welcome! If you have an idea for a new feature or have found a bug, please open an issue or submit a pull request on the GitHub repository.

### License

`portside` is dual-licensed under the MIT License and the Unlicense, at your option.

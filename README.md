# status_cli

A status line for [Claude Code](https://code.claude.com/docs/en/statusline) and the
[Antigravity CLI](https://www.antigravity.google/docs/cli/statusline) (`agy`). It shows how much of your usage
limits you have spent, how close the conversation is to auto-compact and how long the session has been running,
right-aligned at the bottom of the terminal.

```
Opus - High ╱ sessão ━━━━━━━━── 82% ↻ 2m ╱ semana ━━━━━───── 53% ⇥ 2d2h ╱ compactar ━━━━────── 37% ╱ tempo ◷ 00:44
```

A single static binary written in Rust. It needs no runtime, makes no network calls and finishes in under a
millisecond of its own work.

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.sh | sh
```

Windows (PowerShell 5.1 or 7):

```powershell
irm https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.ps1 | iex
```

The installer picks the binary for your system, checks its SHA-256 against the release, places it in
`~/.local/bin` (adding it to `PATH` on Windows) and runs `status_cli setup`. Setup adds the `statusLine` entry to
the settings of every host it finds and keeps a `.bak-status_cli` copy of the previous file. Open a new session
afterwards.

To install by hand, download the binary for your platform from the
[latest release](https://github.com/Isllanrx/status_cli/releases/latest), put it on `PATH` as `status_cli` and run
`status_cli setup`.

## What it shows

| Segment | Meaning |
| --- | --- |
| `Opus - High` | Model and reasoning effort as reported by the host, plus `· fast` in fast mode |
| `sessão` | 5-hour usage window. `↻` is the time until it resets; `⇥` appears when the current pace would use it up before then |
| `semana` | 7-day usage window, same markers |
| `compactar` | How far the context is from auto-compact |
| `tempo` | Session duration |

On agy, `cota` replaces both windows with the quota of the current model and `contexto` shows raw context usage.
Usage limits only exist on Pro and Max plans and appear after the first reply; until then the segment shows `–`.

Claude Code does not publish its auto-compact threshold. The estimate used here is the context window minus
33k tokens, about 83.5% of 200k; setting `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` lowers it.

## Configuration

Everything works out of the box. These environment variables change the defaults:

| Variable | Effect |
| --- | --- |
| `STATUS_CLI_COLOR` | `truecolor`, `256`, `16` or `none`. Useful inside tmux or screen |
| `STATUS_CLI_ASCII` | Use ASCII instead of Unicode glyphs |
| `NO_COLOR` | Disable colors |
| `STATUS_CLI_LOG` | Path of a JSON Lines log with timings per phase, model, effort and the rendered line |

## Compatibility

Color depth and glyphs adapt to the terminal: 24-bit color where it is advertised (Windows Terminal, iTerm2,
WezTerm, Ghostty, Kitty, Alacritty, Konsole, GNOME Terminal and other VTE terminals), 256 colors on Terminal.app,
tmux and PuTTY, 16 colors and ASCII on the Linux console. Under CJK locales box-drawing characters fall back to
ASCII because those terminals draw them double width.

Every release is tested by running the binary through cmd, PowerShell 5.1 and 7, Git Bash, sh, dash, bash, zsh,
ksh, tcsh, fish and nushell on Windows, Linux and macOS (x64 and ARM), and inside Ubuntu, Debian, Arch, Fedora,
Alpine, openSUSE, Rocky and Amazon Linux containers. The Linux binary is statically linked, so it runs on any
distribution.

Codex CLI cannot run external status line commands. The closest built-in setup goes in `~/.codex/config.toml`:

```toml
[tui]
status_line = ["model-with-reasoning", "five-hour-limit", "weekly-limit", "context-used"]
```

## Performance

| | Linux | Windows |
| --- | --- | --- |
| Process start to exit | 0.6 ms | 5.5 ms |
| Time spent in status_cli | 0.08 ms | 0.18 ms |

Most of the Windows figure is the operating system creating the process. State is a small file per session under
`status_cli/` in `XDG_RUNTIME_DIR`, `%LOCALAPPDATA%`, `XDG_CACHE_HOME` or `~/.cache`. It is read once per refresh,
written only when something changes and removed after seven days.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Each feature in `src/features/` owns its parsing, rendering and tests. Shared code lives in `payload`, `state`,
`style`, `terminal` and `layout`.

## License

MIT

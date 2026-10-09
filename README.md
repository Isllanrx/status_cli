<p align="center">
  <img src="assets/icon.png" alt="status_cli icon" width="160">
</p>

# status_cli

[![CI](https://github.com/Isllanrx/status_cli/actions/workflows/ci.yml/badge.svg)](https://github.com/Isllanrx/status_cli/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Isllanrx/status_cli)](https://github.com/Isllanrx/status_cli/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A status line for [Claude Code](https://code.claude.com/docs/en/statusline), the
[Antigravity CLI](https://www.antigravity.google/docs/cli/statusline) (`agy`) and the
[Codex CLI](https://github.com/openai/codex). It shows how much of your usage limits you have spent, how close the
conversation is to auto-compact and how long the session has been running, right-aligned at the bottom of the
terminal.

![status_cli in Codex, Claude Code and the Antigravity CLI: model and effort, session and weekly usage bars, context or auto-compact progress and session clock](assets/cli.png)

One binary written in Rust, statically linked on Linux. It needs no runtime, makes no network calls and spends
well under a millisecond of its own work per refresh.

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.sh | sh
```

Windows (PowerShell 5.1 or 7, no administrator needed):

```powershell
irm https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.ps1 | iex
```

The installer picks the binary for your system and architecture, checks its SHA-256 against the release, places it
in `~/.local/bin` and runs `status_cli setup`, which configures every host it finds and keeps a `.bak-status_cli`
copy of each file it changes. On Windows the folder is added to `PATH`; on Linux and macOS the host settings point
at the binary's full path, so `PATH` does not matter. Open a new session afterwards.

`status_cli doctor` checks the result: version, `PATH`, each host's settings, the `codex-stt` shortcut and the
detected terminal and language. It exits with 1 when something is missing.

### Install with an AI assistant

Ask Claude Code, Codex, agy or any assistant that can run commands:

> Install status_cli from https://github.com/Isllanrx/status_cli and configure it for me.

The steps it needs are short and safe to repeat:

1. On Linux or macOS run `curl -fsSL https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.sh | sh`;
   on Windows run `irm https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.ps1 | iex` in PowerShell.
2. Run `status_cli doctor` (on Windows, open a new terminal first or call `~/.local/bin/status_cli.exe`) and
   confirm it exits with 0.
3. Tell the user to open a new Claude Code or agy session, or to start Codex with `codex-stt`.

The installer asks no questions, needs no administrator rights and only touches `~/.local/bin` and the host settings,
backing them up first.

To install by hand, download your platform's binary from the
[latest release](https://github.com/Isllanrx/status_cli/releases/latest), save it on `PATH` as `status_cli` and run
`status_cli setup`. To update, run the install command again. To remove it, delete the binary and restore the
`.bak-status_cli` copies.

## Hosts

**Claude Code** runs `status_cli` from `statusLine` in `~/.claude/settings.json` (or `CLAUDE_CONFIG_DIR`) every
second, through Git Bash or PowerShell on Windows. A `statusLine` in a project's `.claude/settings.json` takes
precedence over it. Outside fullscreen mode Claude Code shows notifications on the right of the same row, where they
can cover the end of the line. `subagentStatusLine` uses a different contract and is not supported.

**agy** runs it from `statusLine` in `~/.gemini/antigravity-cli/settings.json`. agy only refreshes the line when
the agent changes state, so the clock moves with activity rather than every second; it counts from the first entry
of the conversation transcript.

**Codex CLI** cannot run an external status line, so status_cli starts Codex for you. Use `codex-stt` wherever
you would type `codex`; arguments pass through unchanged:

```sh
codex-stt
codex-stt --model gpt-6-luna
```

`codex-stt` is a shortcut that setup places next to the binary (`status_cli codex` does the same). Codex runs in a
pseudo-terminal one row shorter than the window (ConPTY on Windows, openpty elsewhere) and the line is drawn on the
last row of the same terminal:

- keys, mouse, paste and resizes pass straight through, and clicks on the status row are ignored;
- the line is only drawn between Codex frames, so it never splits an escape sequence, link or synchronized update;
- Codex's scroll regions are kept off the reserved row, also across alternate-screen switches;
- the data comes from the session this launch started in the current folder (resumed sessions included, sub-agents
  ignored), with limits matched by window length and context computed the way Codex shows it; until Codex writes
  that session, the line shows the configured model and the account's latest limits;
- Codex's exit code is returned and the terminal modes are restored even if it crashes.

`status_cli codex --once` prints the line once for tmux status bars or shell prompts. Setup also enables Codex's
native status line in `config.toml`, keeping comments and other settings, for when Codex is started directly.

## What it shows

| Segment | Meaning |
| --- | --- |
| `Opus - High` | Model and reasoning effort as reported by the host, plus `· fast` in fast mode |
| `sessão` | 5-hour usage window. `↻` is the time until it resets; `⇥` appears when the current pace would use it up first |
| `semana` | Weekly usage window, same markers |
| `compactar` | Claude Code only: how far the context is from auto-compact |
| `contexto` | agy and Codex: share of the context window in use |
| `tempo` | Session duration |

Model names and effort levels come straight from the host, so new models need no update. On agy the windows are
those of the current model's family (`gemini-5h` and `gemini-weekly` for Gemini, `3p-…` for third-party models).
Usage limits only exist on subscription plans and appear after the first reply; until then a segment shows `–`.

Claude Code does not send its auto-compact threshold, so `compactar` estimates it the way the documentation
describes it: the auto-compact window minus 33k tokens (about 967k on a 1M window). The window is the model's
context unless `CLAUDE_CODE_AUTO_COMPACT_WINDOW` sets a smaller one, and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` can only
bring the threshold down. Values set through `autoCompactWindow` in settings are not read.

## Languages

Labels follow the system language: English, Spanish, Portuguese, French and Chinese, with English for anything
else. `STATUS_CLI_LANG` overrides it (`en`, `es`, `pt`, `fr`, `zh`); otherwise `LC_ALL`, `LC_MESSAGES` and `LANG`
are used, and the Windows display language when those are empty.

| | en | es | pt | fr | zh |
| --- | --- | --- | --- | --- | --- |
| 5-hour window | session | sesión | sessão | session | 会话 |
| Weekly window | week | semana | semana | semaine | 本周 |
| Auto-compact | compact | compactar | compactar | compacter | 压缩 |
| Context | context | contexto | contexto | contexte | 上下文 |
| Duration | time | tiempo | tempo | temps | 时长 |

Chinese characters count as two columns, so the line keeps its width.

## Configuration

Everything works out of the box. These environment variables change the defaults:

| Variable | Effect |
| --- | --- |
| `STATUS_CLI_COLOR` | `truecolor`, `256`, `16` or `none`, for terminals that do not advertise their depth (tmux, screen) |
| `STATUS_CLI_ASCII` | ASCII instead of Unicode glyphs |
| `STATUS_CLI_LANG` | `en`, `es`, `pt`, `fr` or `zh` |
| `NO_COLOR` | No colors |
| `STATUS_CLI_LOG` | JSON Lines log with per-phase timings, host, model, effort and the rendered line |
| `STATUS_CLI_CODEX` | Program to run instead of `codex` in `codex-stt` |

## Compatibility

Colors and glyphs adapt to the terminal: 24-bit color where it is advertised (Windows Terminal, iTerm2, WezTerm,
Ghostty, Kitty, Alacritty, Konsole, GNOME Terminal and other VTE terminals), 256 colors on Terminal.app, tmux and
PuTTY, and 16 colors with ASCII glyphs on the Linux console. Under CJK locales box drawing falls back to ASCII,
since those terminals draw it double width. The line never exceeds the terminal width.

Every release runs the binary through cmd, PowerShell 5.1 and 7, Git Bash, sh, dash, bash, zsh, ksh, tcsh, fish and
nushell on Windows, Linux and macOS (x64 and ARM), inside Ubuntu, Debian, Arch, Fedora, Alpine, openSUSE, Rocky and
Amazon Linux containers, and installs it from the published release on each platform.

## Performance

| | Linux | Windows |
| --- | --- | --- |
| Process start to exit | 0.6 ms | 5.5 ms |
| Time spent in status_cli | 0.07 ms | 0.18 ms |

Most of the Windows figure is the operating system creating the process. State is one small file per session in
`status_cli/` under `XDG_RUNTIME_DIR`, `%LOCALAPPDATA%`, `XDG_CACHE_HOME` or `~/.cache`, read once per refresh,
written only when something changes and removed after seven days.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo llvm-cov --summary-only
```

The suite has unit tests beside each module and integration tests in `tests/`: `cli` (rendering), `matrix` (every
model, effort and mode), `i18n` (every language), `setup`, `codex`, `wrap` (Codex inside a real pseudo-terminal),
`regression` (one test per fixed bug) and `e2e` (install, configure, then run the configured command through the
host's shell). CI runs them on every platform and reports coverage.

Each feature in `src/features/` owns its parsing, rendering and tests; `payload`, `state`, `style`, `terminal` and
`layout` are shared. Releases are automatic: bump `version` in `Cargo.toml` and merge to `main`. Once every test,
shell and distro check passes, CI tags the commit, publishes the binaries with `SHA256SUMS` and installs them on
each platform.

## License

[MIT](LICENSE)

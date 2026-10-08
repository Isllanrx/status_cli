# status_cli

Right-aligned status line for [Claude Code](https://code.claude.com/docs/en/statusline) and the
[Antigravity CLI](https://www.antigravity.google/docs/cli/statusline) (`agy`), written in Rust. It reads the host's
JSON payload on stdin and prints one ANSI line. No network, no runtime dependencies, ~3 ms of CPU per refresh.

```
Opus - High ╱ sessão ━━━━━━━━─│ 79% ↻ 12m ╱ semana ━━━│━───── 52% ╱ compactar ━━━─────── 30% ╱ tempo ◴ 00:33
```

| Segment | Claude Code | agy |
| --- | --- | --- |
| Model - effort | `model.display_name` (first word) and `effort.level` | `model.display_name` and `execution_mode` |
| `sessão` / `cota` | 5-hour window and time to reset | quota of the current model, or the most consumed one |
| `semana` | 7-day window | — |
| `compactar` / `contexto` | progress towards auto-compact | context window usage |
| `tempo` | session duration as an animated `hh:mm` clock | time since the conversation was first seen |

## Behavior

- **Colors** follow a mint → amber → coral gradient as usage grows.
- **Pace marker `│`** shows how much of the quota window has already elapsed. A fill past the marker means usage is
  ahead of time.
- **Animations**: the clock hand spins and the colon blinks every second; minutes are underlined on the second they
  turn; values at 90% or more pulse. When a value changes, bar and number ease towards it over 3 s (ease-out) and the
  bar edge flashes while it grows. Hosts refresh at most once per second, so a transition has about three frames.
- **Auto-compact threshold** is not exposed by Claude Code. The estimate is the window minus 33k tokens (20k reserved
  for output plus a 13k buffer), about 83.5% of 200k. `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` can only lower it.
- **Session state** (clock anchor and last values for transitions) lives in `status_cli-<session>` inside
  `XDG_RUNTIME_DIR` or the temp directory. It is rewritten only when something changes; files older than 7 days are
  removed. The clock keeps ticking between host events by extrapolating from the last reported duration.
- **Layout** pads with U+2800 (blank braille) because hosts trim leading whitespace. Width comes from `COLUMNS`
  (Claude Code) or `terminal_width` (agy), minus 8 columns for the host's indentation. When the line does not fit,
  bars shrink (10, 7, 5, none) and finally labels and separators are dropped.
- `rate_limits` only exists for Pro/Max plans and after the first response of a session; missing values show `–`.
- agy quota resets are read from `reset_in_seconds`; `reset_time` is not parsed.

## Terminal compatibility

The host captures the output and draws it, so any shell can launch the binary; what varies between terminals is
color depth and glyph support. Detection uses the environment the host passes on:

| Signal | Result |
| --- | --- |
| `STATUS_CLI_COLOR=truecolor\|256\|16\|none` | forces a color depth |
| `NO_COLOR` set, or `TERM=dumb` | no colors |
| `COLORTERM=truecolor\|24bit`, `WT_SESSION`, `TERM_PROGRAM` of iTerm2, WezTerm, Ghostty, VS Code, Hyper, Tabby, Warp, Rio, `TERM=*-direct`, or Windows without `TERM` | 24-bit color |
| `TERM=linux` | 16 colors and ASCII glyphs |
| anything else (xterm, tmux, screen, Terminal.app, PuTTY, Alacritty without `COLORTERM`) | 256 colors |
| `STATUS_CLI_ASCII` set | ASCII glyphs (`=`, `-`, `\|`, `/`, `~`) |

Inside tmux or GNU Screen, set `STATUS_CLI_COLOR=truecolor` when the outer terminal supports it. In ASCII mode the
padding is plain spaces, which hosts trim, so the line is left-aligned.

On Windows the binary works the same from cmd, PowerShell 5.1, PowerShell 7 and Git Bash; the UTF-8 BOM that Windows
PowerShell 5.1 adds when piping to native programs is ignored.

## Install

Download a binary from the releases page, or build it:

```sh
cargo build --release
```

Put `status_cli` (`status_cli.exe` on Windows) in a directory on `PATH`, such as `~/.local/bin`, and reference it by
name. A bare name works no matter which shell the host uses; a quoted path is not executed by PowerShell.

Claude Code, `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "status_cli", "refreshInterval": 1 } }
```

agy, `~/.gemini/antigravity-cli/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "status_cli", "enabled": true } }
```

agy only refreshes on agent state changes, so its clock moves with activity.

Codex CLI has no command-backed status line; its closest equivalent is the built-in item list in
`~/.codex/config.toml`:

```toml
[tui]
status_line = ["model-with-reasoning", "five-hour-limit", "weekly-limit", "context-used"]
status_line_use_colors = true
```

## Observability

Set `STATUS_CLI_LOG` to a file path to append one JSON line per run:

```json
{"ts":1791435540086,"us":412,"host":"claude","session":"d3c07f4a-...","error":null}
```

The file rotates to `<name>.1` past 1 MiB. Without the variable nothing is written. Parse errors are also printed on the
line itself as `status_cli: <error>`, and the process always exits 0 so the host never breaks.

## Security

- Input is capped at 1 MiB; session ids are reduced to `[A-Za-z0-9_-]` before being used in file names.
- Only the clock anchor and gauge values are stored; e-mail and other payload fields are never persisted.
- The Windows binary imports only `kernel32`, `ntdll` and the C runtime: no network, registry, process creation or
  code injection APIs. Release binaries are unsigned; SmartScreen may warn on first run of a downloaded file.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs these on Linux, macOS and Windows and builds binaries for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
`x86_64-apple-darwin` and `x86_64-pc-windows-msvc`. Tags `v*` publish a release with `SHA256SUMS`.

Windows binary from Linux (requires `gcc-mingw-w64-x86-64`):

```sh
rustup target add x86_64-pc-windows-gnu
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release --target x86_64-pc-windows-gnu
```

The source is organized by feature (`src/features/{model,quota,context,clock,telemetry}.rs`), each holding its
payload mapping, rendering and tests; `payload`, `state`, `style`, `terminal` and `layout` are shared.

## License

MIT

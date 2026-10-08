# status_cli

Right-aligned status line for [Claude Code](https://code.claude.com/docs/en/statusline) and the
[Antigravity CLI](https://www.antigravity.google/docs/cli/statusline) (`agy`), in Rust. No network, no runtime
dependencies.

```
Opus - High ╱ sessão ━━━━━━━━── 82% ↻ 2m ╱ semana ━━━━━───── 53% ⇥ 2d2h ╱ compactar ━━━━────── 37% ╱ tempo ◷ 00:44
```

- **sessão / semana**: 5-hour and 7-day quotas, time to reset (`↻`) and, when the current pace runs out before the
  reset, time to exhaustion (`⇥`). On agy: `cota`, the current model's quota.
- **compactar**: progress towards auto-compact, estimated as the window minus 33k tokens; `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
  can only lower it. On agy: `contexto`, raw context usage.
- **tempo**: session duration as an animated clock, kept ticking between host events.
- Values ease towards changes over 3 s, growing bars flash at the edge, values at 90%+ pulse.

## Install

Download a binary from the releases page (static Linux musl, universal macOS, Windows x64/arm64) or run
`cargo build --release`, and put it on `PATH`. Reference it by bare name so it runs from any shell:

```json
{ "statusLine": { "type": "command", "command": "status_cli", "refreshInterval": 1 } }
```

That goes in `~/.claude/settings.json`; for agy use `~/.gemini/antigravity-cli/settings.json` with
`"enabled": true` instead of `refreshInterval`. On macOS, a downloaded binary may need
`xattr -d com.apple.quarantine status_cli`. Codex CLI has no command-backed status line.

## Compatibility

The host draws the output, so what matters is the terminal's color depth and font. Detection reads the environment:

| Terminal | Colors |
| --- | --- |
| Windows Terminal, conhost, WSL, mintty (Git Bash, MSYS2, Cygwin), iTerm2, WezTerm, Ghostty, Kitty, Alacritty, Foot, Konsole, VTE (GNOME Terminal, Tilix, Terminator, Xfce), Warp, Tabby, Hyper, VS Code | 24-bit |
| Terminal.app, ConEmu/Cmder, PuTTY, tmux, `screen-256color`, other xterm | 256 |
| Linux virtual console, plain `screen` | 16 |
| `NO_COLOR`, `TERM=dumb` | none |

ASCII glyphs replace Unicode on the Linux console, under CJK locales (where box drawing is double width) or with
`STATUS_CLI_ASCII`. `STATUS_CLI_COLOR=truecolor|256|16|none` forces a depth, e.g. inside tmux. Only basic SGR
attributes are used: no cursor movement or blinking, so nothing can corrupt the host's screen.

CI runs the binary through cmd, PowerShell 5.1 and 7, Git Bash, sh, dash, bash, zsh, ksh, tcsh, fish and nushell on
Windows x64/arm64, Linux x64/arm64 and macOS Intel/Apple Silicon, plus Ubuntu, Debian, Arch, Fedora, Alpine,
openSUSE, Rocky and Amazon Linux containers.

## Performance

| | Linux (static) | Windows |
| --- | --- | --- |
| Spawn to exit | 0.65 ms | ~6 ms (process creation) |
| Work inside the binary | ~0.1 ms | ~0.2 ms |

State is one small file per session (`status_cli-<id>` in `XDG_RUNTIME_DIR` or the temp dir), read once per
refresh, rewritten only on change and pruned after 7 days.

## Observability

`STATUS_CLI_LOG=<file>` appends one JSON line per run (`ts`, `us`, `host`, `session`, `error`), rotating at 1 MiB.
Errors also show on the line as `status_cli: <error>`; the process always exits 0.

## Development

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Code is organized by feature in `src/features/`; `payload`, `state`, `style`, `terminal` and `layout` are shared.

## License

MIT

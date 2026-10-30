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

`status_cli doctor` checks the result: version, `PATH`, each host's settings and usage hooks, whether `codex`
runs through status_cli (Windows) and the detected terminal and language. It exits with 1 when something is missing.

`status_cli update` moves to the latest GitHub release. It compares the release tag with the running version and stops
if there is nothing newer. Otherwise it downloads that release's binary for your system with `curl`, which ships with
Windows 10 and later, macOS and most Linux distributions. It checks the SHA-256 against the release's `SHA256SUMS`,
makes sure the new binary runs, and only then swaps it in place of the running one. Open sessions pick it up on their
next refresh. On Windows the running binary can't be overwritten, so it is renamed to `status_cli.old`, which the next
update deletes. Errors exit with 1 and leave the installed binary untouched. `STATUS_CLI_RELEASE_URL` (release JSON)
and `STATUS_CLI_BASE_URL` (asset folder) point it at another source, which is how the tests run it against a local
release.

### Install with an AI assistant

Ask Claude Code, Codex, agy or any assistant that can run commands:

> Install status_cli from https://github.com/Isllanrx/status_cli and configure it for me.

The steps it needs are short and safe to repeat:

1. On Linux or macOS run `curl -fsSL https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.sh | sh`;
   on Windows run `irm https://raw.githubusercontent.com/Isllanrx/status_cli/main/install.ps1 | iex` in PowerShell.
2. Run `status_cli doctor` (on Windows, open a new terminal first or call `~/.local/bin/status_cli.exe`) and
   confirm it exits with 0.
3. Tell the user to open a new Claude Code or agy session. On Windows, for `codex` to show the line, the user runs
   `status_cli setup` once from an elevated PowerShell (see Codex CLI below) and opens a new terminal.

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

**agy** runs it from `statusLine` in `~/.gemini/antigravity-cli/settings.json`. The clock counts from when the
`agy` window opened: status_cli walks up its parent processes to `agy` and reads that process's start time (Toolhelp
on Windows, `ps` on Linux and macOS). If `agy` isn't among its parents, it counts from the conversation's first
transcript entry instead. agy only runs the command when the agent changes state and has no refresh interval, so while
the agent sits idle the line keeps the last value it drew. Idle time is not lost: the next refresh shows the full
time since the window opened.

**Codex CLI** cannot run an external status line, so status_cli starts Codex for you; arguments pass through
unchanged:

```sh
codex
codex --model gpt-6-luna
```

Codex has no external status line item (its `[tui] status_line` only takes built-in items, see
`codex-rs/tui/src/bottom_pane/status_line_setup.rs`) and its npm launcher has no override for the binary it starts,
so the only way to keep typing `codex` is to intercept the command on `PATH`. On Windows, `status_cli setup` run from
an elevated PowerShell copies itself to `%ProgramFiles%\status_cli\codex.exe` (writable only by administrators) and
inserts that folder in the machine `PATH` right before the first folder holding a Codex launcher (npm's
`AppData\Roaming
pm` is usually in the machine `PATH`, which Windows puts before the user `PATH`). The previous value
is saved once to `%ProgramFiles%\status_cli\machine-path.bak`, the value keeps its `REG_EXPAND_SZ` type and is read
and written through the registry API, and open programs are told the environment changed; new terminals pick it up.
Without elevation setup only reports what it would do. PowerShell 7, Windows PowerShell, cmd and Git Bash then all
resolve `codex` to the shim. The shim hands the call to the `status_cli` found on `PATH` (so `status_cli update`
keeps it current) with `STATUS_CLI_SHIM` set; the wrapper starts the real Codex with that folder removed from its
`PATH` and `STATUS_CLI_IN_CODEX=1` set. If either variable is already present when the shim runs (a `codex` started
from inside Codex, or an older status_cli that doesn't strip the folder), the shim runs the real Codex directly,
without the wrapper, so it can never call itself. If the `status_cli` on `PATH` can't be started, the shim runs the
wrapper itself, and if the pseudo-terminal or console mode can't be set up, the wrapper runs Codex directly without
the line, so `codex` always opens. Codex's own files, settings, sandbox and approvals are not touched.
To undo it, restore the machine `PATH` from the backup and delete the folder. The old `codex-stt` shortcut is removed
by setup; `status_cli codex` still runs the wrapper explicitly, which is the way on Linux and macOS. Subcommands that
don't open the TUI (`exec`, `login`, `mcp`, `--help`,
`--version` and the like) run Codex directly, without the wrapper; `resume` and `fork` are wrapped. Codex runs in a
pseudo-terminal one row shorter than the window (ConPTY on Windows, openpty elsewhere) and the line is drawn on the
last row of the same terminal:

- keys, mouse, paste and resizes pass straight through, and clicks on the status row are ignored;
- the line is only drawn between Codex frames, so it never splits an escape sequence, link or synchronized update;
- Codex's scroll regions are kept off the reserved row, also across alternate-screen switches;
- the data comes from the session this launch started in the current folder (resumed sessions included, sub-agents
  ignored, and so are rollouts whose `session_meta.source` is not `cli` or `exec`, such as Codex Desktop's
  `vscode` ones, which carry no window or limits), with the model and effort from the session's last
  `turn_context` (Codex writes it once per change, so it is read incrementally past the 256 KB tail), limits matched by window length and context computed the way Codex shows it (12k baseline over
  `model_context_window`; when a `token_count` carries no window, the usable window Codex derives from
  `models_cache.json` for the model, `context_window` or `config.toml`'s `model_context_window` capped at
  `max_context_window`, times `effective_context_window_percent`); until Codex writes
  that session, the line shows the configured model and the account's latest limits. Among several `codex`
  launches in the same folder, each one prefers a session created after it started, so two windows don't share a
  session. The clock counts from the later of Codex's session start and the launch, so `codex resume` counts this
  window, not the original session;
- Codex's exit code is returned and the terminal modes are restored even if it crashes.

`status_cli codex --once` prints the line once for tmux status bars or shell prompts. Setup turns off Codex's own
status line (`[tui] status_line = []` in `config.toml`, keeping comments and other settings), so only this line is
shown; Codex falls back to its default footer items when the key is absent.

## What it shows

| Segment | Meaning |
| --- | --- |
| `Opus 5.5 - High` | Model name with its version as the host reports it (a trailing `(…)` such as `(1M context)` or `(High)` is dropped), reasoning effort, and `· fast` in fast mode |
| `sessão` | 5-hour usage window. `↻` is the time until it resets; `⇥` appears when the current pace would use it up first |
| `semana` | Weekly usage window, same markers |
| `compactar` | Claude Code only: how far the context is from auto-compact |
| `contexto` | agy and Codex: share of the context window in use |
| `tempo` | Time since the session's window opened |

Each host reports usage from its own last API response, so a terminal that has been idle for a while would show an
older percentage than one in use. status_cli keeps the latest reading of each window in a shared file per host and
account (`usage-claude.json`, `usage-codex.json`, `usage-agy.json` in the cache folder; `CLAUDE_CONFIG_DIR` and
`CODEX_HOME` get their own file). The file also keeps what each session last reported; a session whose reading
changed (or a session reporting for the first time) has just heard from the server, so its reading becomes the shared
value, while a session repeating an old reading does not. Usage can go down inside a window (Codex rollouts show
drops such as 97% to 93% with the same reset time), so the latest change wins, not the highest value. A reading with
a reset time more than 15 minutes later starts a new window, and expired windows and sessions are dropped. Every open
terminal then shows the same value, and a new session shows it before its first response. A window whose reset time
has passed shows 0% until the host reports the new one, instead of the usage from before the reset.

Model names and effort levels come straight from the host, so new models need no update. On agy, which bucket
belongs to which model comes from agy itself: its status line payload only lists buckets by id, so status_cli keeps
the groups that `agy -p "/usage" --output-format json` reports (for example "Gemini Models: Gemini Flash, Gemini Pro"
with `gemini-5h` and `gemini-weekly`, "Claude and GPT models: Claude Opus, Claude Sonnet, GPT-OSS" with `3p-…`, and
"Gemini 4 Argon" with only `gemini-argon-wtu-weekly`) in `agy-quota-groups.json` in the cache folder. That query costs
no quota and starts no conversation, but takes 3 to 4 seconds, close to the 5 seconds agy gives the status line, so it
runs as a detached `status_cli agy-groups` when the file is missing, older than an hour or lacks a bucket the payload
shows, at most once every two minutes (`agy-quota-groups.lock`). The model is matched to the group whose listed model
(or group name) has all its words in the model name. Until the file exists, or for a model no group lists, the bucket
prefix sharing the most words with the model name is used instead. agy always shows `sessão` and `semana`: a group
without a 5-hour bucket (Argon) or with a bucket agy marks `disabled` (the 5-hour limit of a group whose weekly limit
is used up "does not currently apply") shows `–` there. The segment follows a model switch on the next run, which agy
only triggers when the agent changes state.
Usage limits only exist on subscription plans and appear after the first reply; until then a segment shows `–`.

Claude Code does not send its auto-compact threshold, and its `used_percentage` is measured against the model's full
window, so `compactar` rebuilds the threshold the way the documentation and changelog describe it: the auto-compact
window minus 33k tokens (967k on a 1M window). The window is resolved in Claude Code's order:
`CLAUDE_CODE_AUTO_COMPACT_WINDOW` (`k`/`M` suffixes, raised to 100k and capped at 1M, ignored when it isn't a number),
then the first settings file that sets one, searched from managed (`managed-settings.d/*.json`, then
`managed-settings.json` in `C:\Program Files\ClaudeCode`, `/Library/Application Support/ClaudeCode` or
`/etc/claude-code`) through the project's `.claude/settings.local.json` and `.claude/settings.json` (from
`workspace.project_dir`) to the user's `settings.json`. In each file the model's `modelSettings` entry (what
`/autocompact` saves; matched by canonical ID, ignoring `[1m]` and date suffixes, `"auto"` meaning the default) beats
the top-level `autoCompactWindow`. The window is capped at the model's context, and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
can only bring the threshold down. With `autoCompactEnabled: false` or `DISABLE_AUTO_COMPACT`, the segment shows the
raw context as `contexto`. Not visible to status_cli: the `--autocompact` launch flag, `--settings`, and managed
settings delivered by MDM, the registry or the claude.ai console.

## Usage limits for agents

`status_cli setup` also hooks `status_cli guard` into every host, so the agent knows how much of the 5-hour and
weekly limits is used and when each one resets, without being asked:

| Host | Where | When |
| --- | --- | --- |
| Claude Code | `hooks` in `settings.json` | every prompt (`UserPromptSubmit`), and before `Agent`, `Task` or `Workflow` |
| Codex CLI | `~/.codex/hooks.json` | session start and every prompt |
| agy | plugin in `~/.gemini/config/plugins/status_cli/` | session start, and before each model call near the limit |

The agent reads a line such as `status_cli: session 57% (resets in 58m), week 21% (resets in 5d0h).` From 85% of
the session or 90% of the week it is also told to keep its task notes current and to ask before starting subagents
or workflows. From 95% of the session or 97% of the week it is told not to start new tasks: record what is done
and what comes next, tell the user, and stop. In Claude Code, subagents then need confirmation, and are blocked once
the limit is nearly exhausted. Codex has no confirmation step, so it only blocks.

Claude Code and agy are read from the state the status line saves for that session; Codex is read from the session
file Codex itself writes, so it works with or without the wrapper. A window that already reset is not reported.
Codex runs only hooks you trust: after `status_cli setup`, approve the new ones once with `/hooks` in Codex.
Running setup again replaces the previous guard entries and leaves every other hook alone.

## Languages

Labels follow the system language, with English for anything else. `STATUS_CLI_LANG` overrides it; otherwise
`LC_ALL`, `LC_MESSAGES` and `LANG` are used, and the Windows display language when those are empty. The first two
letters pick the language, so `pt`, `pt_BR` and `pt-PT` all give Brazilian Portuguese and `zh_TW` gives simplified
Chinese.

| Code | Language | 5-hour window | Weekly window | Auto-compact | Context | Duration |
| --- | --- | --- | --- | --- | --- | --- |
| `en` | English | session | week | compact | context | time |
| `pt` | Portuguese (Brazil) | sessão | semana | compactar | contexto | tempo |
| `es` | Spanish | sesión | semana | compactar | contexto | tiempo |
| `zh` | Chinese (simplified) | 会话 | 本周 | 压缩 | 上下文 | 时长 |
| `hi` | Hindi | सत्र | सप्ताह | संक्षेपण | संदर्भ | समय |
| `ar` | Arabic | الجلسة | الأسبوع | الضغط | السياق | الوقت |
| `fr` | French | session | semaine | compacter | contexte | temps |
| `de` | German | Sitzung | Woche | verdichten | Kontext | Zeit |
| `ja` | Japanese | セッション | 今週 | 圧縮 | コンテキスト | 時間 |
| `ru` | Russian | сессия | неделя | сжатие | контекст | время |
| `ko` | Korean | 세션 | 주간 | 압축 | 컨텍스트 | 시간 |
| `it` | Italian | sessione | settimana | compattare | contesto | tempo |
| `id` | Indonesian | sesi | minggu | ringkas | konteks | waktu |
| `tr` | Turkish | oturum | hafta | sıkıştır | bağlam | süre |
| `vi` | Vietnamese | phiên | tuần | nén | ngữ cảnh | thời gian |
| `th` | Thai | เซสชัน | สัปดาห์ | บีบอัด | บริบท | เวลา |
| `bn` | Bengali | সেশন | সপ্তাহ | সংকোচন | প্রসঙ্গ | সময় |
| `ur` | Urdu | سیشن | ہفتہ | کمپیکٹ | سیاق | وقت |
| `pl` | Polish | sesja | tydzień | kompaktowanie | kontekst | czas |
| `nl` | Dutch | sessie | week | compacteren | context | tijd |

Chinese, Japanese and Korean characters count as two columns and combining marks (Thai, Hindi, Bengali, Arabic
vowel signs) as none, so the line keeps its width. Arabic and Urdu are written in logical order; how they read
depends on the terminal's right-to-left support. The guard's messages to the agent use the same language.

## Configuration

Everything works out of the box. These environment variables change the defaults:

| Variable | Effect |
| --- | --- |
| `STATUS_CLI_COLOR` | `truecolor`, `256`, `16` or `none`, for terminals that do not advertise their depth (tmux, screen) |
| `STATUS_CLI_ASCII` | ASCII instead of Unicode glyphs |
| `STATUS_CLI_LANG` | Language code from the table above, such as `en`, `pt`, `zh`, `ja` or `ar` |
| `NO_COLOR` | No colors |
| `STATUS_CLI_LOG` | JSON Lines log with per-phase timings, host, model, effort and the rendered line |
| `STATUS_CLI_CODEX` | Program the wrapper runs instead of the real `codex` |

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

The code is split by slice, and each slice owns its parsing, rendering and tests:

| Folder | Contents |
|---|---|
| `src/statusline/` | Line segments (`model`, `context`, `quota`, `clock`) and the width-based `layout` |
| `src/statusline/clock/` | Session clock and the process lookup that dates the agy window |
| `src/agy/` | Official agy quota groups from `agy -p /usage`, cached and refreshed in the background |
| `src/claude/` | Claude Code auto-compact window from the environment and settings files |
| `src/codex/` | Codex rollout reader, model window, launch arguments; `wrap/` runs Codex inside a pseudo-terminal |
| `src/usage/` | Usage shared across terminals and the `guard` hook |
| `src/install/` | `setup`, `doctor`, host hooks, the PowerShell `codex` function and `update` |
| `src/shared/` | Host payload, session state, time parsing and telemetry |
| `src/render/` | Colors, gauges and terminal capabilities |
| `src/i18n/` | Labels in every language |

The suite has unit tests beside each module and one integration crate per slice in `tests/`, sharing `tests/common/`:
`statusline` (`cli` rendering, `matrix` with every model, effort and mode, `i18n` with every language, `regression`
with one test per fixed bug), `codex` (`reader`, and `wrap` with Codex inside a real pseudo-terminal), `install`
(`setup`, `update`, and `e2e`, which installs, configures and runs the configured command through the host's shell)
and `usage` (`guard` hooks and their registration). CI runs them on every platform and reports coverage.

Releases are automatic. Every push to `main` that changes `src/`, `Cargo.toml` or `Cargo.lock`
since the last `vX.Y.Z` tag ships the next minor version: `v1.1.0` becomes `v1.2.0`. CI writes that number into
`Cargo.toml` and `Cargo.lock` inside the runner before building, so nothing is committed back. The `version` in
`Cargo.toml` is a floor: raise it by hand for a major or a specific version, and CI uses it when it's higher. Pushes
that only touch docs or CI don't release. Once every test, shell and distro check passes, CI tags the commit,
publishes the binaries with `SHA256SUMS` and installs them on each platform. Local builds report the `Cargo.toml`
version, which can trail the latest release.

## License

[MIT](LICENSE)

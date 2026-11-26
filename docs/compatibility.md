# Host compatibility

What status_cli depends on in each host, where the host defines it, and what to re-check when a host updates.
Verified on 2026-11-25 against Claude Code (CHANGELOG 2.1.332, `anthropics/claude-code` at `a5e8375`), Antigravity
CLI 1.5.17 (`google-antigravity/antigravity-cli` at `279437d`) and Codex CLI 0.166.0 (`openai/codex` at `5148e3d6`).
Keep shallow clones of the three repositories side by side in one folder. Claude Code and agy publish only a
CHANGELOG and examples, so their documentation is the contract; Codex is open source and `codex-rs` is the contract.

## Claude Code

Docs: [statusline](https://code.claude.com/docs/en/statusline.md), [model-config](https://code.claude.com/docs/en/model-config.md),
[hooks](https://code.claude.com/docs/en/hooks.md), [settings](https://code.claude.com/docs/en/settings.md),
[env-vars](https://code.claude.com/docs/en/env-vars.md).

| status_cli reads | Contract | If it changes |
| --- | --- | --- |
| `session_id` | statusline "Available data"; stable per session, new after `/clear` | per-session state and shared usage source |
| `model.id`, `model.display_name` | documented fields; the display format is not | `short_name` keeps words before `(`; `model.id` keys `modelSettings` |
| `effort.level`, `fast_mode` | documented; `effort` absent for models without levels | badge only |
| `rate_limits.five_hour`/`seven_day` `used_percentage`, `resets_at` (epoch seconds) | documented; only on subscription plans, after the first reply; removed after `resets_at` | `Sessão`/`Semana`; a millisecond `resets_at` is converted |
| `context_window.used_percentage`, `context_window_size` | measured against the full model window, not the compact threshold | `Compactar` rebuilds the threshold |
| `cost.total_duration_ms` | "accumulates across resumes and doesn't include time while the session isn't running" | `Tempo` caps it by the age of the parent `claude` process (Windows, Linux) |
| `workspace.project_dir` | documented | project `.claude/settings*.json` lookup |
| `COLUMNS` env | width for the script; the payload has no width | layout; status_cli keeps 8 columns spare (docs ask for 4) |
| triggers | start/resume, new assistant message, `/compact`, permission mode, vim, `refreshInterval` (≥ 1 s), `resets_at`; 300 ms debounce, a new trigger cancels a running script | the clock relies on `refreshInterval: 1` |
| auto-compact | `CLAUDE_CODE_AUTO_COMPACT_WINDOW` (100k–1M, below clamps to 100k, above to 1M, unparsable ignored) > `--autocompact` > `/autocompact` (`modelSettings[model].autoCompactWindow`) > `autoCompactWindow`; threshold = window − 33k | `--autocompact` and `--settings` can't be seen from the status line |
| guard hook | `UserPromptSubmit` and `PreToolUse` (`Agent|Task|Workflow`); `hookSpecificOutput.additionalContext` ≤ 10 000 chars; a timeout drops the context; exit 2 blocks | `status_cli guard` always exits 0 |

Not read and safe to ignore: `session_name`, `prompt_cache`, `thinking`, `rate_limits.spend_limit`, `worktree`,
`workspace.repo`, `pr`, `agent`, `vim`, `output_style`, `version`.

## Antigravity CLI (agy)

Docs: [statusline](https://antigravity.google/docs/cli/statusline), settings, hooks, plugins under
https://antigravity.google/docs/cli/.

| status_cli reads | Contract | If it changes |
| --- | --- | --- |
| invocation | `cmd /c` on Windows (`sh -c` elsewhere), JSON on stdin, at most every 300 ms, killed after 5 s, 30 failures in a row disable the line; only on agent state changes (quota also refreshes during turns ≥ 1 min since 1.5.16) | no animation on agy; exit code always 0 |
| `product` = `antigravity`, `conversation_id` (= `session_id`, empty before the first conversation) | documented | host detection, state key `agy-<id>` |
| `model.id`/`display_name`/`effort` | documented; `id` equals the display name (not a slug); `effort` only for models with levels | group matching, badge |
| `quota.<bucket>` `remaining_fraction`, `reset_time`, `reset_in_seconds`, `disabled` | documented; also `remaining_amount` (not read); an unused window comes as `remaining_fraction: 1`, `reset_time: 1970-01-01T00:00:00Z` | `Sessão`/`Semana`; the 1970 sentinel clears the shared reading |
| `agy -p /usage --output-format json` → `command.data.groups[].{name, description "Models within this group: …", buckets[].{id, window}}` | implied by CHANGELOG 1.1.11/1.2.15, not documented as JSON | bucket → group mapping; run it from PowerShell or `Command::new`, never Git Bash (MSYS rewrites `/usage` into a path and agy runs a real turn) |
| bucket ids `gemini-5h`, `gemini-weekly`, `3p-5h`, `3p-weekly` (`gemini-argon-wtu-weekly` on some plans) | not documented; differ by plan | an unknown id refreshes the groups |
| process tree `status_cli ← cmd.exe ← agy.exe` | single native `%LOCALAPPDATA%\agy\bin\agy.exe` | `Tempo`; fallback: newest `~/.gemini/antigravity-cli/log/cli-*.log` naming the conversation, then the transcript's first `created_at` |
| `~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl` | path documented by hooks | clock fallback |
| plugin `~/.gemini/config/plugins/status_cli/{plugin.json,hooks.json}` (`SessionStart`, `PreInvocation`) | documented; hooks in settings.json are not loaded | guard; `agy plugin validate` warns that the name is not kebab-case |

## Codex CLI

Source: `codex-rs` in the clone.

| status_cli reads | Upstream | If it changes |
| --- | --- | --- |
| `$CODEX_HOME` or `~/.codex` | `utils/home-dir` | sessions not found |
| `sessions/YYYY/MM/DD/rollout-*.jsonl`, `{timestamp, type, payload}` per line | `rollout/src`, `history` (`RolloutItem`) | everything; rollouts older than 7 days become `.jsonl.zst` (ignored) |
| `session_meta.payload` `id`, `timestamp`, `cwd`, `source`, `originator`, `parent_thread_id` | `protocol.rs` `SessionMeta`, `SessionSource` (`cli`, `vscode` default, `exec`, `mcp`, objects for `custom`/`internal`/`subagent`) | terminal sessions through the app-server daemon are `vscode` + `originator: codex-tui`; the daemon's originator comes from the first client that initialized it, `codex_cli_rs` when that was remote control |
| `turn_context.payload` `model`, `effort` | `TurnContextItem`, once per user turn | model before the next turn |
| `event_msg` `thread_settings_applied` → `thread_settings.model`, `.reasoning_effort` | `ThreadSettingsAppliedEvent`, written on `/model`, effort change and resume | immediate model switch |
| `event_msg` `token_count` → `info.last_token_usage.total_tokens`, `info.model_context_window`, `rate_limits.{limit_id, primary, secondary}.{used_percent, window_minutes, resets_at}` | `TokenCountEvent`, `RateLimitSnapshot`; `limit_id` defaults to `codex` | context and limits; a 30-day free-plan window shows as `Semana` |
| context % (12k baseline) | `tui/src/token_usage.rs`, `protocol.rs` `BASELINE_TOKENS` | `Contexto` |
| `models_cache.json` `models[].slug, context_window, max_context_window, effective_context_window_percent` | `models-manager` | context window fallback |
| `config.toml` `model`, `model_reasoning_effort`, `model_context_window`, `[tui] status_line` | `config/src/config_toml.rs`, `types.rs`; `[]` or missing brings the native footer back | line before the first message; `["raw-output"]` keeps the footer empty |
| TUI size | `backend.size()` of the ConPTY the wrapper opens one row shorter | the reserved row |
| subcommands | `cli/src/main.rs` `enum Subcommand` | a new subcommand would be wrapped in the PTY; keep `SUBCOMMANDS` in `src/codex/launch.rs` in sync |
| npm launcher | `codex-cli/bin/codex.js` resolves the native binary from its package, not from `PATH` | the shim only needs `codex` on `PATH` |

## Re-check on every update

```powershell
$up = 'C:\src\upstream'
git -C "$up\claude-code" pull --ff-only
git -C "$up\antigravity-cli" pull --ff-only
git -C "$up\codex" pull --ff-only
Select-String -Path "$up\claude-code\CHANGELOG.md" -Pattern 'status ?line|statusline|refreshInterval|rate_limits|context_window|total_duration|COLUMNS|autocompact|AUTO_COMPACT|modelSettings|UserPromptSubmit|additionalContext|effort' | Select-Object -First 40
Select-String -Path "$up\antigravity-cli\CHANGELOG.md" -Pattern 'status ?line|statusLine|quota|/usage|Orchestrator|Effort|agent_state|hook|Windows' | Select-Object -First 40
agy -p /usage --output-format json
Select-String -Path "$up\codex\codex-rs\protocol\src\protocol.rs" -Pattern 'pub enum SessionSource|pub struct SessionMeta|pub struct TurnContextItem|pub struct ThreadSettingsSnapshot|pub struct TokenUsageInfo|pub struct RateLimitSnapshot|BASELINE_TOKENS'
Select-String -Path "$up\codex\codex-rs\cli\src\main.rs" -Pattern '^\s+[A-Z][A-Za-z]+\(' | Select-Object -First 60
status_cli doctor
```

`$up` is the folder holding the clones.

Then compare against `src/shared/payload.rs`, `src/agy/mod.rs`, `src/codex/` and run the dev_box suite. For a new
agy or Claude Code payload field, capture a real payload into a temporary cache folder (set `LOCALAPPDATA` and
`STATUS_CLI_NO_AUTO_UPDATE=1`) so the real usage store is never touched.

## Known gaps

- Codex: after `/new`, `/fork` or `/resume` inside an open window the wrapper keeps reading the first rollout it
  settled on, and two `codex` windows in the same folder can pick up each other's session before the first message.
  Fixing both needs a claim on each rollout by the wrapper that reads it.
- Codex on Windows starts `cmd /d /c codex <args>`: an argument with `&` or `|` and no spaces is split by `cmd`.
- `status_cli --once`/`--watch` for Codex count the clock from the session start, the wrapper from the launch.
- agy: `remaining_amount` buckets show no value; `cycle_mode` is not shown; a `quota` map with one malformed bucket
  is dropped whole.
- Claude Code on macOS: no parent-process lookup without starting `ps` every second, so a resumed session shows the
  accumulated `total_duration_ms`.
- Codex: if the app-server daemon was first initialized by remote control, terminal sessions carry the
  `codex_cli_rs` originator and are not read.
- The guard reads the session gauges by their translated label, so a language change orphans the previous reading
  until its window resets.

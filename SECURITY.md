# Security Policy

## Supported versions

Only the latest release on the [releases page](https://github.com/Isllanrx/status_cli/releases/latest) gets fixes.
`status_cli update` moves an installed copy to it.

## Reporting a vulnerability

Report it privately through GitHub: **Security → Report a vulnerability** on this repository
(private vulnerability reporting is enabled). Please do not open a public issue for a security problem.

Include the status_cli version (`status_cli --version`), the OS and terminal, the host (Claude Code, Codex or agy) and
the steps or payload that reproduce it. A confirmed issue is fixed in a new release and disclosed in a GitHub
security advisory once that release is out.

## What status_cli touches

Knowing this helps judge whether a behavior is a vulnerability:

- **Input**: the host's JSON on stdin (capped at 1 MiB), Codex rollout files under `CODEX_HOME`, and Claude Code
  settings files (managed, project and user) to find the auto-compact window. Model names and other host text are
  stripped of control characters before they reach the terminal, so a payload cannot inject escape sequences.
- **Files written**: per-session state and shared usage readings in the cache folder (`status_cli` under
  `XDG_RUNTIME_DIR`, `LOCALAPPDATA` or `XDG_CACHE_HOME`), written atomically. `status_cli setup` edits the hosts'
  settings and hook files, keeping the user's other keys.
- **Machine-wide change (Windows, opt-in)**: `status_cli setup` from an elevated shell copies the binary to
  `%ProgramFiles%\status_cli\codex.exe`, a folder only administrators can write, and adds that folder to the machine
  `PATH` only when a `codex` is already on `PATH`. The previous value is saved to `machine-path.bak` in that folder.
  Without elevation, or with `STATUS_CLI_NO_SHIM=1`, nothing machine-wide changes.
- **Processes started**: the real `codex` (with the shim folder removed from its `PATH` and `STATUS_CLI_IN_CODEX=1`,
  so it cannot start the shim again) and, for agy, `agy -p /usage --output-format json` in the temp folder, which
  reads quota groups without running a model turn.
- **Font (installers only)**: `install.ps1` and `install.sh` download `CascadiaCode.tar.xz` from the pinned
  Nerd Fonts release (v3.5.1, `ryanoasis/nerd-fonts`), check it against that release's `SHA-256.txt` and copy four
  `.ttf` files to the user's font folder (no administrator). On Windows they set Windows Terminal's default font only
  when it is unset or a stock Cascadia font, after saving `settings.json.bak-status_cli`. `STATUS_CLI_NO_FONT=1`
  skips all of it.
- **Network**: only `status_cli update` and the install scripts, which download from GitHub releases over HTTPS.
  The status line itself never connects anywhere and sends no telemetry; `STATUS_CLI_LOG` writes a local file only.

## Release integrity

Release binaries are built by GitHub Actions. `status_cli update`, `install.ps1` and
`install.sh` check each download against the `SHA256SUMS` file published with the same release and stop on a
mismatch. That check catches a corrupted or swapped asset but comes from the same release, so it is not a signature:
anyone able to publish a release could replace both.

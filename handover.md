# Prompt Wallet engineering handover

This document is for the next coding agent continuing Prompt Wallet. Start here, then read [`README.md`](README.md) for the user-facing behavior.

## Repository state

- Target repository: `https://github.com/marcykay404/prompts-wallet`
- Local Windows path: `C:\Users\mahch\Documents\Codex\pw-cli`
- Local WSL path: `/mnt/c/Users/mahch/Documents/Codex/pw-cli`
- Local branch: `codex/initial-cli`
- CLI command: **`pwt`**. Do not rename it to `pw`; that name conflicts with BSD and another community application.
- Rust package name: `prompts-wallet`
- Remote state: no implementation has been committed or pushed. The remote `main` branch still contains only the initial MIT license commit.
- Local state: the implementation files are currently untracked. Run `git status --short` before doing anything so they are not overlooked.

The user explicitly requested that incomplete work not be pushed. Keep changes local while implementing and verifying additional scope. Confirm the intended handoff before making remote changes.

## Current product scope

The local v0.1 implementation is working and includes:

- A bounded inline terminal UI; it does not use the alternate screen.
- 10, 20, and 40-line viewport sizes, cycled with `v` or `Alt-V` in text-entry screens.
- A Home screen containing the five most frequently used prompts.
- Fuzzy search across titles, aliases, tags, and bodies.
- Relevance-first search ordering, with usage frequency and recency as tie-breakers.
- Ordered extraction and collection of `{{variables}}`.
- Non-recursive variable substitution with multiline paste support.
- Clipboard integration through `arboard`, followed by platform fallbacks:
  - WSL: `clip.exe`
  - macOS: `pbcopy`
  - Linux Wayland: `wl-copy`
  - Linux X11: `xclip`
- Copy-and-stay, copy-and-exit, and print-and-exit actions.
- A one-line exit status after the inline interface is removed.
- New/edit workflows using the configured external editor.
- Temporary drafts, validation before replacement, and preservation of invalid drafts.
- Human-readable Markdown prompt files with TOML frontmatter.
- JSON usage persistence with atomic writes.
- Platform-specific storage and a portable `PW_HOME` override.
- Non-interactive `list`, `show`, `copy`, `new`, `edit`, and `paths` commands.

## UX invariants

Treat these as requirements unless the user explicitly changes them:

1. Invoking `pwt` must render immediately below the shell command, not take over the full terminal.
2. On exit, clear only the lines owned by `pwt`, restore normal terminal input and cursor visibility, and leave one concise status line.
3. `q` quits on navigation screens. In Search, `q` is search text; `Esc` returns Home and `Ctrl-C` always exits.
4. Usage is recorded only after a successful clipboard copy or print. Opening, previewing, editing, or cancelling is not usage.
5. Exact/relevant matches must beat weak but frequently used matches.
6. Repeated variables are requested once, in first-appearance order.
7. Replacement values are not recursively interpreted as templates.
8. Invalid edits must never overwrite the last valid prompt. Preserve the draft and report its path.
9. Prompt IDs are stable and cannot be changed during an edit.
10. Prompt files are the source of truth. Usage state is separate from prompt content.

## Runtime flow

```text
pwt
 └─ Home: top five by usage
     ├─ 1–5 / Enter → Variables, or Preview if no variables
     ├─ s → Search → Variables/Preview
     ├─ n → title/tags → external editor → validate → Home
     ├─ e → external editor → validate → Home
     ├─ v → cycle viewport height
     └─ q → clean inline region → print final status → shell

Preview
 ├─ c → copy → record usage → Home
 ├─ C → copy → record usage → exit
 ├─ p → print → record usage → exit
 ├─ e → edit source prompt
 └─ b/Esc → Variables or originating list
```

## Architecture map

| File | Responsibility |
|---|---|
| `src/main.rs` | Clap commands, dependency wiring, interactive event loop, and side effects |
| `src/app.rs` | UI state machine and side-effect-free actions |
| `src/ui.rs` | Ratatui rendering and inline terminal lifecycle/cleanup |
| `src/model.rs` | Prompt metadata, parsing, validation, and serialization |
| `src/storage.rs` | Platform paths, configuration, vault loading, filenames, and atomic writes |
| `src/search.rs` | Frequent-prompt ordering and fuzzy relevance ranking |
| `src/template.rs` | Variable extraction and non-recursive rendering |
| `src/usage.rs` | Usage counters, timestamps, loading, and persistence |
| `src/editor.rs` | Editor selection, draft creation, validation, and safe replacement |
| `src/clipboard.rs` | Testable clipboard interface and platform backends |
| `tests/cli.rs` | End-to-end tests for non-interactive CLI behavior |
| `examples/prompts/` | Example prompt files |

Side effects deliberately sit outside the `App` state machine. `App::handle_event` returns an `Action`; `main.rs` performs clipboard, editor, storage, resize, or exit work and feeds the result back into application state. Preserve this boundary so behavior remains testable without touching a real clipboard or editor.

## Storage format

Prompt files use Markdown with TOML frontmatter:

```markdown
+++
id = "d91f5f9e-1820-4db3-8960-bbaf02850d3e"
title = "Review code for security issues"
tags = ["code", "security"]
aliases = ["security review"]
+++
Review this {{language}} code:

{{code}}
```

With `PW_HOME` set, all data is relocated together:

```text
$PW_HOME/
├── config.toml
├── prompts/
├── drafts/
└── usage.json
```

Example configuration:

```toml
editor = ["code", "--wait"]
viewport_lines = 10
```

The editor command is stored as an argument array to avoid shell-quoting and injection problems. Resolution order is configuration, `$VISUAL`, `$EDITOR`, then `vi`.

## Verification baseline

The latest local verification completed successfully:

- 26 library/unit tests
- 4 end-to-end CLI tests
- 30 total tests, all passing
- `clippy` with warnings denied
- Rust formatting check
- Git whitespace check
- Locked optimized release build
- Manual terminal checks for inline rendering, 10/20-line resizing, variable entry, preview, print, cleanup, and final status
- Process-level editor check for draft creation and promotion into the prompt vault

Run the normal checks:

```bash
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
git diff --check
```

The user's Rust toolchain is installed in WSL. In a restricted coding-agent session, the existing Cargo registry cache may be read-only. If that occurs, point `CARGO_HOME` and `CARGO_TARGET_DIR` at writable temporary directories; do not install another system-wide toolchain.

For changes affecting the terminal lifecycle, test in a real terminal or terminal emulator. Unit tests cannot prove cursor restoration or preservation of shell history. Verify normal exit, `Ctrl-C`, errors, editor handoff, and viewport resizing.

## Meaningful test expectations

Do not replace behavioral tests with implementation-detail assertions. New work should test observable invariants, especially:

- Search relevance versus frequency and recency.
- Usage recorded after success, never before.
- Prompt round-tripping and stable identity.
- Invalid edits preserving the original and recoverable draft.
- Multiline and repeated variable behavior.
- State transitions for every new key binding.
- Portable `PW_HOME` behavior.
- Non-interactive command output and exit status.
- Rendering of critical controls in the compact 10-line viewport.

Keep external processes behind traits, following `Clipboard` and `PromptEditor`, so tests do not alter the developer's clipboard or launch a real editor.

## Known limitations

These are not regressions unless their behavior changes unexpectedly:

- macOS compatibility is designed through cross-platform crates and `pbcopy`, but was not run on macOS in the current environment.
- A real clipboard copy was not automated because it would overwrite the user's clipboard. The interface is covered with a fake and the terminal print path was exercised manually.
- Direct `pwt show` and `pwt copy` leave template variables unresolved; interactive mode collects values.
- There is no archive/delete workflow yet.
- There is no built-in multiline prompt editor; editing intentionally delegates to the user's editor.
- Invalid prompt files are skipped with a warning. A malformed `usage.json` currently stops startup rather than recovering automatically.
- Existing prompt filenames are not renamed when their titles change; identity remains correct because usage is keyed by UUID.
- Concurrent `pwt` processes can race while writing usage or the same prompt. There is no file locking.
- The vault is scanned on startup; there is no persistent search index or file watcher.
- Sync, encryption, and shared/collaborative vaults are not implemented.
- Native Windows is not a current target; WSL, Linux, and macOS are.

## Suggested continuation backlog

### High value

1. Add CI for Linux and macOS with formatting, tests, Clippy, and release builds.
2. Add archive/restore rather than immediate deletion; require confirmation and make recovery obvious.
3. Add a visible draft-recovery screen for failed or interrupted edits.
4. Add file locking or conflict-safe usage merging for concurrent processes.
5. Test clipboard behavior manually on both WSL and macOS and document any required packages.

### Product improvements

1. Optional variable defaults, for example `{{language|Rust}}`, with backward-compatible parsing.
2. A non-interactive way to supply variables to `show` and `copy`.
3. Pinned prompts alongside frequency-ranked prompts.
4. Import/export and validation commands such as `pwt doctor`.
5. Better scoring for multi-token queries while retaining exact-match precedence.
6. Optional recency decay so lifetime-heavy prompts do not dominate forever.

### Later integrations

1. Git-based synchronization for a private prompt vault.
2. Google Drive sync using only narrow `drive.appdata` or `drive.file` scopes.
3. Conflict copies or an operation log rather than last-writer-wins replacement.
4. Optional encryption for synchronized prompts; clearly document clipboard exposure.
5. Packaging through Homebrew and a Linux install script after the file format stabilizes.

## Definition of done for future work

Before declaring additional scope complete:

1. Preserve the UX and data invariants above or document the approved change.
2. Add behavioral tests for the new path and its failure modes.
3. Run formatting, the locked full test suite, strict Clippy, and a release build.
4. Manually exercise terminal behavior when rendering, keyboard handling, cleanup, editor handoff, or clipboard behavior changes.
5. Update `README.md` and this handover when commands, storage, architecture, limitations, or verification counts change.
6. Review `git status` carefully because the initial implementation began as untracked files.
7. Do not push partially working changes.

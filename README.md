# Prompt Wallet

Prompt Wallet (`pwt`) is a fast, local-first prompt picker for WSL, Linux, and macOS. It opens as a small inline interface beneath the command you typed, lets you find and fill a prompt, copies the rendered result, then cleans up after itself.

```text
$ pwt
┌ Prompt Wallet ────────────────────────────────┐
│ › 1  Review code for security issues  used 31│
│   2  Explain code simply              used 22│
│                                               │
│ ↑↓ select  Enter/1–5 open  s search           │
│ n new  e edit  v size  q quit                 │
└───────────────────────────────────────────────┘
```

After exit, the interface is removed and only a status remains:

```text
$ pwt
✓ Copied “Review code for security issues” to clipboard
$
```

## Status

This repository contains an initial working implementation. Prompt loading, frequency ranking, fuzzy search, variable substitution, external editing, portable storage, clipboard integration, the inline UI, and non-interactive commands are implemented. The project is still pre-release and its file format may evolve.

## Install and run

Rust 1.85 or newer is recommended.

```bash
cargo build
cargo run --bin pwt
```

Install it for the current user:

```bash
cargo install --path .
pwt
```

## Interactive keys

### Home

- `1`–`5` or `Enter`: open a frequent prompt
- `↑` / `↓`: change selection
- `s`: fuzzy search
- `n`: create a prompt
- `e`: edit the highlighted prompt
- `v`: cycle the inline viewport through 10, 20, and 40 lines
- `q`: exit

### Search

- Type to update results immediately
- `↑` / `↓`: change selection
- `Enter`: open
- `Ctrl-E`: edit the highlighted result
- `Alt-V`: resize while the search field owns normal letter keys
- `Esc`: return Home

### Variables and preview

- Variables use `{{variable_name}}` syntax and are requested once in first-seen order
- `Enter`: accept a variable and continue
- `Shift-Tab`: go to the previous variable
- `Ctrl-R`: render immediately
- `c`: copy and return Home
- `C`: copy and exit
- `p`: print and exit
- `e`: edit the source prompt
- `b` / `Esc`: go back

Pasted multiline values are preserved. Substitution is deliberately non-recursive: braces inside a replacement value remain literal.

## Adding and editing

`n` collects a title and optional comma-separated tags, then temporarily hands the terminal to your editor. `e` edits a temporary copy of an existing prompt. The draft is validated before the original is atomically replaced; invalid work stays in the drafts directory and the valid original remains untouched.

Editor resolution order:

1. `editor` in `config.toml`
2. `$VISUAL`
3. `$EDITOR`
4. `vi`

Example configuration for VS Code:

```toml
editor = ["code", "--wait"]
viewport_lines = 10
```

## Prompt format

Prompts are Markdown with TOML frontmatter:

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

The stable ID keeps usage history attached when a title or filename changes. See [`examples/prompts/security-review.md`](examples/prompts/security-review.md) for a complete example.

## Storage and portability

`pwt paths` prints the resolved locations. By default, Prompt Wallet follows the platform's normal configuration, data, and state directories.

Set `PW_HOME` to keep every wallet file together or place it in a synchronized folder:

```bash
export PW_HOME="$HOME/my-prompt-wallet"
pwt
```

The portable layout is:

```text
$PW_HOME/
├── config.toml
├── prompts/
├── drafts/
└── usage.json
```

Usage is counted only after a successful clipboard copy or print. Opening or cancelling a prompt does not affect its ranking.

## Non-interactive commands

```bash
pwt list
pwt show "security review"
pwt copy "security review"
pwt new --title "Daily standup" --tags work,writing
pwt edit "security review"
pwt paths
```

## Tests

```bash
cargo test
```

The suite covers prompt parsing and round-tripping, validation and draft recovery, ordered variable extraction, multiline and non-recursive rendering, fuzzy relevance versus frequency, usage persistence, state transitions, portable paths, and end-to-end CLI behavior.

## License

MIT

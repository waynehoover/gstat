# git-status-watch

Reactive git status for your terminal. Watches your repo with native filesystem events (FSEvents/inotify) and outputs structured status instantly — no polling.

## Why

Every existing tool for git status in your terminal (gitmux, tmux-gitbar, shell prompt plugins) works the same way: poll `git status` on a timer. Your status bar updates every 2-5 seconds whether anything changed or not, and misses changes between intervals.

`git-status-watch` flips this around. It uses native filesystem events to watch your repo and outputs a line only when something actually changes. This means:

- **Instant updates** — you see changes the moment they happen, not seconds later
- **Zero wasted work** — no CPU spent re-running `git status` when nothing changed
- **Works everywhere** — a single compiled binary that outputs to stdout, so it plugs into any shell prompt, tmux, zellij, or anything else that can read a line of text
- **Two modes** — `--once` for per-prompt freshness (like gitmux), watch mode for reactive updates between keypresses

A shell prompt can use both: `--once` on each Enter for immediate accuracy, plus a background watcher that triggers repaints when external changes happen (IDE saves, background fetches, other terminals).

## Install

```sh
brew install waynehoover/tap/git-status-watch
```

Or via Cargo:

```sh
cargo install git-status-watch
```

## Usage

```
git-status-watch [OPTIONS] [PATH]
```

Also works as a git subcommand:

```
git status-watch [OPTIONS] [PATH]
```

**Options:**

| Flag | Description |
|---|---|
| `--format <STR>` | Custom format string (see placeholders below) |
| `--once` | Print once and exit |
| `--state-dir <DIR>` | Write status to a file in this directory (keyed by repo path) |
| `--debounce-ms <MS>` | Debounce window in milliseconds (default: 75) |
| `--always-print` | Print on every filesystem event, even if unchanged |

By default, `git-status-watch` outputs JSON and keeps running, printing a new line whenever the git status changes.

### Placeholders

| Placeholder | Description |
|---|---|
| `{branch}` | Branch name or short detached hash |
| `{staged}` | Staged file count |
| `{modified}` | Modified file count |
| `{untracked}` | Untracked file count |
| `{conflicted}` | Conflicted file count |
| `{ahead}` | Commits ahead of upstream |
| `{behind}` | Commits behind upstream |
| `{stash}` | Stash count |
| `{state}` | Operation state: merge, rebase, cherry-pick, bisect, revert, or empty |

Format strings support `\t` and `\n` escape sequences for tab and newline.

### Examples

One-shot JSON:

```sh
git-status-watch --once
# {"branch":"main","staged":0,"modified":2,"untracked":1,"conflicted":0,"ahead":1,"behind":0,"stash":0,"state":"clean"}
```

One-shot with custom format:

```sh
git-status-watch --once --format '{branch} +{staged} ~{modified} ?{untracked}'
# main +0 ~2 ?1
```

Watch mode (prints on each change):

```sh
git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}'
# main +0 ~2 ?1 ⇡1⇣0
# ... (updates reactively as files change)
```

## Shell Integration

### Starship

Add a custom module to `~/.config/starship.toml`. Disable the built-in git modules to avoid duplicate info:

```toml
[git_branch]
disabled = true

[git_status]
disabled = true

[custom.gitstatus]
command = "git-status-watch --once --state-dir /tmp/gsw --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}'"
when = "git rev-parse --show-toplevel"
require_repo = true
format = "[$output]($style) "
style = "bold purple"
```

### Fish (with Tide)

Use `--once` in a custom Tide item for immediate status on Enter, plus a background watcher for reactive updates:

```fish
# ~/.config/fish/functions/_tide_item_gitstatus.fish
function _tide_item_gitstatus
    set -l raw (command git-status-watch --once --state-dir /tmp/gsw --format \
        '{branch}\t{staged}\t{modified}\t{untracked}\t{conflicted}\t{ahead}\t{behind}\t{stash}\t{state}' 2>/dev/null)
    test -n "$raw"; or return

    set -l fields (string split \t $raw)
    set -l branch $fields[1]
    # ... parse remaining fields, render with _tide_print_item
end
```

```fish
# ~/.config/fish/conf.d/gitstatus.fish — reactive repaints between keypresses
status is-interactive; or return

function _gitstatus_repaint --on-variable _gitstatus_signal_$fish_pid
    commandline -f repaint
end

function _gitstatus_on_prompt --on-event fish_prompt
    set -l repo_root (command git rev-parse --show-toplevel 2>/dev/null)
    # start/restart git-status-watch in background, bump
    # _gitstatus_signal_$fish_pid on each stdout line to trigger repaint
end
```

### Zsh

Background watcher with `TRAPUSR1` for reactive prompt refresh:

```zsh
__gsw_line=""

_git_status_watch_start() {
  if [[ -z "$__GSW_PID" ]] || ! kill -0 "$__GSW_PID" 2>/dev/null; then
    git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
      __gsw_line="$line"
      kill -USR1 $$ 2>/dev/null
    done &
    __GSW_PID=$!
    disown
  fi
}

TRAPUSR1() { zle && zle reset-prompt }
precmd_functions+=(_git_status_watch_start)
RPROMPT='${__gsw_line}'
```

### Bash

```bash
__gsw_line=""

_git_status_watch_start() {
  if [[ -z "$__GSW_PID" ]] || ! kill -0 "$__GSW_PID" 2>/dev/null; then
    git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
      echo "$line" > /tmp/gsw_$$
    done &
    __GSW_PID=$!
    disown
  fi
  [[ -f /tmp/gsw_$$ ]] && __gsw_line=$(cat /tmp/gsw_$$)
}

PROMPT_COMMAND="_git_status_watch_start; $PROMPT_COMMAND"
PS1='\u@\h \w ${__gsw_line} \$ '
```

## Tmux

Use `--once` for polling (like gitmux):

```tmux
set -g status-right '#(git-status-watch --once --format " {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}")'
set -g status-interval 2
```

Or use watch mode for reactive updates (no polling):

```sh
# In your shell startup:
if [[ -n "$TMUX" ]]; then
  git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
    | while IFS= read -r line; do echo "$line" > /tmp/gsw_tmux; done &
  disown
fi
```

```tmux
set -g status-right '#(cat /tmp/gsw_tmux 2>/dev/null)'
set -g status-interval 1
```

## Zellij (zjstatus)

Pipe directly into [zjstatus](https://github.com/dj95/zjstatus) for a reactive status bar:

```fish
# fish
function __zellij_git_status_watch --on-event fish_prompt
    set -q ZELLIJ; or return
    set -l repo_root (git rev-parse --show-toplevel 2>/dev/null)
    # manage watcher lifecycle, pipe to zjstatus:
    git-status-watch --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' "$repo_root" \
        | while read -l line
            zellij pipe "zjstatus::pipe::pipe_git_status::$line"
        end &
end
```

```zsh
# zsh
_zellij_git_status_watch() {
  [[ -n "$ZELLIJ" ]] || return
  local repo_root=$(git rev-parse --show-toplevel 2>/dev/null)
  git-status-watch --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' "$repo_root" | while IFS= read -r line; do
    zellij pipe "zjstatus::pipe::pipe_git_status::${line}"
  done &
  disown
}
precmd_functions+=(_zellij_git_status_watch)
```

## How It Works

1. Resolves the git repo root from the current directory (or a path argument)
2. Computes and prints initial status immediately
3. Watches `.git/` and the worktree recursively via native filesystem events ([notify](https://docs.rs/notify))
4. Debounces events (75ms default), filters to only relevant `.git/` state files (HEAD, index, refs, sentinel files)
5. On change: recomputes status, compares to previous, prints only if different
6. Exits cleanly on broken pipe (consumer closed)

Status is computed by shelling out to git:
- `git status --porcelain=v2 --branch --no-optional-locks` for branch, upstream, file counts
- Stash reflog line count (`.git/logs/refs/stash`) for stash count — no subprocess needed
- Sentinel file checks (`.git/MERGE_HEAD`, etc.) for operation state

When using `--state-dir`, multiple instances coordinate via `flock`: only one watches the repo (leader), others watch the state file (followers). This means N terminals = 1 `git status` call per change, not N.

## License

MIT

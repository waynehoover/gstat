# git-status-watch

Reactive git status for your terminal. Watches your repo with native filesystem events (FSEvents/inotify) and outputs structured status instantly — no polling.

## Install

```sh
cargo install --path .
```

Or build from source:

```sh
git clone https://github.com/waynehoover/gstat.git
cd gstat
cargo install --path .
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

### Examples

One-shot JSON (replacement for shell scripts):

```sh
git-status-watch --once
# {"branch":"main","staged":0,"modified":2,"untracked":1,"conflicted":0,"ahead":1,"behind":0,"stash":0,"state":"clean"}
```

One-shot with custom format:

```sh
git-status-watch --once --format '{branch} +{staged} ~{modified} ?{untracked}'
# main +0 ~2 ?1
```

Watch mode (prints a line on every change):

```sh
git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}'
# main +0 ~2 ?1 ⇡1⇣0
# ... (updates as files change)
```

## Shell Integration

### Fish

Reactive git status in your prompt — updates without waiting for the next prompt:

```fish
function fish_prompt
    # your prompt here, reading from a global variable
    echo (set -q __gsw_line; and echo $__gsw_line; or echo '')
end

function __git_status_watch_start --on-event fish_prompt
    if not set -q __GSW_PID; or not kill -0 $__GSW_PID 2>/dev/null
        git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
            | while read -l line
                set -g __gsw_line $line
            end &
        set -g __GSW_PID (jobs -lp | tail -1)
        disown 2>/dev/null
    end
end
```

### Zsh

Background watcher feeding a variable, re-rendered on update via `TRAPUSR1`:

```zsh
__gsw_line=""

_git_status_watch_start() {
  if [[ -z "$__GSW_PID" ]] || ! kill -0 "$__GSW_PID" 2>/dev/null; then
    git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
      __gsw_line="$line"
      kill -USR1 $$ 2>/dev/null  # signal zsh to refresh prompt
    done &
    __GSW_PID=$!
    disown
  fi
}

TRAPUSR1() {
  zle && zle reset-prompt
}

precmd_functions+=(_git_status_watch_start)

# Use $__gsw_line in your PROMPT or RPROMPT:
RPROMPT='${__gsw_line}'
```

### Bash

Similar approach using `PROMPT_COMMAND`:

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

Pipe into a tmux status line. Add to `~/.tmux.conf`:

```tmux
set -g status-right '#(git-status-watch --once --format " #{branch} +#{staged} ~#{modified} ?#{untracked} ⇡#{ahead}⇣#{behind}")'
set -g status-interval 5
```

For a reactive (non-polling) approach, run in watch mode writing to a tmpfile:

```sh
# In your shell startup (.bashrc, .zshrc, etc.):
if [[ -n "$TMUX" ]]; then
  if [[ -z "$__GSW_PID" ]] || ! kill -0 "$__GSW_PID" 2>/dev/null; then
    git-status-watch --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
      | while IFS= read -r line; do echo "$line" > /tmp/gsw_tmux; done &
    __GSW_PID=$!
    disown
  fi
fi
```

```tmux
# ~/.tmux.conf
set -g status-right '#(cat /tmp/gsw_tmux 2>/dev/null)'
set -g status-interval 1
```

## Zellij (zjstatus)

Pipe directly into [zjstatus](https://github.com/dj95/zjstatus) for a reactive status bar. The function tracks the current repo and restarts the watcher when you `cd` between repos:

```fish
# fish
function __zellij_git_status_watch --on-event fish_prompt
    set -q ZELLIJ; or return

    set -l repo_root (git rev-parse --show-toplevel 2>/dev/null)

    # Kill watcher if we changed repos (or left a repo)
    if set -q __GSW_PID; and test "$repo_root" != "$__GSW_REPO"
        kill $__GSW_PID 2>/dev/null
        set -e __GSW_PID
        set -e __GSW_REPO
        if test -z "$repo_root"
            zellij pipe "zjstatus::pipe::pipe_git_status::"
            return
        end
    end

    # Start watcher if not running
    if not set -q __GSW_PID; or not kill -0 $__GSW_PID 2>/dev/null
        set -e __GSW_PID
        if test -n "$repo_root"
            git-status-watch --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' "$repo_root" \
                | while read -l line
                    zellij pipe "zjstatus::pipe::pipe_git_status::$line"
                end &
            set -g __GSW_PID (jobs -lp | tail -1)
            set -g __GSW_REPO "$repo_root"
            disown 2>/dev/null
        end
    end
end
```

```zsh
# zsh
_zellij_git_status_watch() {
  if [[ -n "$ZELLIJ" ]]; then
    local repo_root
    repo_root=$(git rev-parse --show-toplevel 2>/dev/null)

    # Kill watcher if we changed repos
    if [[ -n "$__GSW_PID" && "$repo_root" != "$__GSW_REPO" ]]; then
      kill "$__GSW_PID" 2>/dev/null
      unset __GSW_PID __GSW_REPO
      if [[ -z "$repo_root" ]]; then
        zellij pipe "zjstatus::pipe::pipe_git_status::"
        return
      fi
    fi

    if [[ -z "$__GSW_PID" ]] || ! kill -0 "$__GSW_PID" 2>/dev/null; then
      unset __GSW_PID
      if [[ -n "$repo_root" ]]; then
        git-status-watch --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' "$repo_root" | while IFS= read -r line; do
          zellij pipe "zjstatus::pipe::pipe_git_status::${line}"
        done &
        __GSW_PID=$!
        __GSW_REPO="$repo_root"
        disown
      fi
    fi
  fi
}
precmd_functions+=(_zellij_git_status_watch)
```

## How It Works

1. Resolves the git repo root from the current directory (or a path argument)
2. Computes and prints initial status immediately
3. Watches `.git/` and the worktree recursively via native filesystem events ([notify](https://docs.rs/notify))
4. Debounces events (75ms default), filters to only relevant `.git/` state files (HEAD, index, refs)
5. On change: recomputes status, compares to previous, prints only if different
6. Exits cleanly on broken pipe (consumer closed)

Status is computed by shelling out to git:
- `git status --porcelain=v2 --branch` for branch, upstream, file counts
- `git rev-list --count refs/stash` for stash count
- Sentinel file checks (`.git/MERGE_HEAD`, etc.) for operation state

## License

MIT

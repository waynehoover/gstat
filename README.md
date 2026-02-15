# gstat

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
gstat [OPTIONS] [PATH]
```

**Options:**

| Flag | Description |
|---|---|
| `--format <STR>` | Custom format string (see placeholders below) |
| `--once` | Print once and exit |
| `--debounce-ms <MS>` | Debounce window in milliseconds (default: 75) |
| `--always-print` | Print on every filesystem event, even if unchanged |

By default, gstat outputs JSON and keeps running, printing a new line whenever the git status changes.

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
gstat --once
# {"branch":"main","staged":0,"modified":2,"untracked":1,"conflicted":0,"ahead":1,"behind":0,"stash":0,"state":"clean"}
```

One-shot with custom format:

```sh
gstat --once --format '{branch} +{staged} ~{modified} ?{untracked}'
# main +0 ~2 ?1
```

Watch mode (prints a line on every change):

```sh
gstat --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}'
# main +0 ~2 ?1 ⇡1⇣0
# ... (updates as files change)
```

## Shell Integration

### Fish

Reactive git status in your prompt — updates without waiting for the next prompt:

```fish
function fish_prompt
    # your prompt here, reading from a universal variable
    echo (set -q __gstat_line; and echo $__gstat_line; or echo '')
end

function __gstat_watch --on-event fish_prompt
    if not set -q __GSTAT_PID; or not kill -0 $__GSTAT_PID 2>/dev/null
        gstat --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
            | while read -l line
                set -g __gstat_line $line
            end &
        set -g __GSTAT_PID (jobs -lp | tail -1)
        disown 2>/dev/null
    end
end
```

### Zsh

Background gstat feeding a variable, re-rendered on update via `TRAPUSR1`:

```zsh
__gstat_line=""

_gstat_watch() {
  if [[ -z "$__GSTAT_PID" ]] || ! kill -0 "$__GSTAT_PID" 2>/dev/null; then
    gstat --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
      __gstat_line="$line"
      kill -USR1 $$ 2>/dev/null  # signal zsh to refresh prompt
    done &
    __GSTAT_PID=$!
    disown
  fi
}

TRAPUSR1() {
  zle && zle reset-prompt
}

precmd_functions+=(_gstat_watch)

# Use $__gstat_line in your PROMPT or RPROMPT:
RPROMPT='${__gstat_line}'
```

### Bash

Similar approach using `PROMPT_COMMAND`:

```bash
__gstat_line=""

_gstat_watch() {
  if [[ -z "$__GSTAT_PID" ]] || ! kill -0 "$__GSTAT_PID" 2>/dev/null; then
    gstat --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
      echo "$line" > /tmp/gstat_$$
    done &
    __GSTAT_PID=$!
    disown
  fi
  [[ -f /tmp/gstat_$$ ]] && __gstat_line=$(cat /tmp/gstat_$$)
}

PROMPT_COMMAND="_gstat_watch; $PROMPT_COMMAND"
PS1='\u@\h \w ${__gstat_line} \$ '
```

## Tmux

Pipe gstat into a tmux status line. Add to `~/.tmux.conf`:

```tmux
set -g status-right '#(gstat --once --format " #{branch} +#{staged} ~#{modified} ?#{untracked} ⇡#{ahead}⇣#{behind}")'
set -g status-interval 5
```

For a reactive (non-polling) approach, run gstat in watch mode writing to a tmpfile:

```sh
# In your shell startup (.bashrc, .zshrc, etc.):
if [[ -n "$TMUX" ]]; then
  if [[ -z "$__GSTAT_PID" ]] || ! kill -0 "$__GSTAT_PID" 2>/dev/null; then
    gstat --format '{branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
      | while IFS= read -r line; do echo "$line" > /tmp/gstat_tmux; done &
    __GSTAT_PID=$!
    disown
  fi
fi
```

```tmux
# ~/.tmux.conf
set -g status-right '#(cat /tmp/gstat_tmux 2>/dev/null)'
set -g status-interval 1
```

## Zellij (zjstatus)

Pipe gstat directly into [zjstatus](https://github.com/dj95/zjstatus) for a reactive status bar:

```fish
# fish
function __zellij_gstat --on-event fish_prompt
    if set -q ZELLIJ; and not set -q __GSTAT_PID; or not kill -0 $__GSTAT_PID 2>/dev/null
        gstat --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' \
            | while read -l line
                zellij pipe "zjstatus::pipe::pipe_git_status::$line"
            end &
        set -g __GSTAT_PID (jobs -lp | tail -1)
        disown 2>/dev/null
    end
end
```

```zsh
# zsh
_zellij_gstat() {
  if [[ -n "$ZELLIJ" ]]; then
    if [[ -z "$__GSTAT_PID" ]] || ! kill -0 "$__GSTAT_PID" 2>/dev/null; then
      gstat --format ' {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}' | while IFS= read -r line; do
        zellij pipe "zjstatus::pipe::pipe_git_status::${line}"
      done &
      __GSTAT_PID=$!
      disown
    fi
  fi
}
precmd_functions+=(_zellij_gstat)
```

## How It Works

1. Resolves the git repo root from the current directory (or a path argument)
2. Computes and prints initial status immediately
3. Watches `.git/` and the worktree recursively via native filesystem events ([notify](https://docs.rs/notify))
4. Debounces events (75ms default), filters out noisy `.git/objects/` and `.git/logs/` paths
5. On change: recomputes status, compares to previous, prints only if different
6. Exits cleanly on broken pipe (consumer closed)

Status is computed by shelling out to git:
- `git status --porcelain=v2 --branch` for branch, upstream, file counts
- `git rev-list --count refs/stash` for stash count
- Sentinel file checks (`.git/MERGE_HEAD`, etc.) for operation state

## License

MIT

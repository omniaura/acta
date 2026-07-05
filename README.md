# Acta

**A terminal multiplexer for agentic coding** — like tmux, but built for AI agents.

```bash
acta new claude            # Spin up Claude Code in a detached session
acta ls                    # List sessions
acta attach 1              # Attach (detach again with Ctrl-\)
acta cb next               # Pop the next agent-pushed snippet onto your clipboard
acta ws sync               # Clone/refresh every repo in your workspace
```

Acta is three tools in one binary:

1. **Detachable sessions** — run any coding agent in a PTY owned by a
   detached daemon. Close your terminal, drop your SSH connection, come back
   later: the agent kept working. Re-attach from anywhere.
2. **Acta Clipboard** (`acta clipboard` / `acta cb`) — a stateful FIFO queue
   for agent-to-human handoff. Agents `push` commands and snippets; you
   `next` through them straight onto your system clipboard. No more
   multiline heredoc copy/paste hell.
3. **Acta Workspaces** (`acta workspace` / `acta ws`) — manage many repos as
   one workspace using plain clones in a gitignored `clones/` folder:
   the parallel-worktree experience with zero worktree headaches.

**Status:** v0.2.0 — the three pillars above are implemented and usable today.

## Installation

```bash
git clone https://github.com/omniaura/acta
cd acta
cargo install --path .

# Shell completions (bash/zsh/fish/elvish/powershell)
acta completions zsh > "${fpath[1]}/_acta"
```

Requires Rust 1.85+ and git. Linux and macOS.

## Sessions

```bash
# Start an agent in a detached session (any configured plugin, or any raw command)
acta new claude
acta new opencode --name refactor -- --model some-model
acta new bash --name scratch

# Watch and manage
acta ls                    # table of sessions with status/pid/age
acta attach refactor       # by name or id; Ctrl-\ detaches
acta logs refactor -n 50   # peek at output without attaching
acta kill refactor         # SIGTERM the agent (add --force for SIGKILL)
acta clean                 # drop records of exited sessions
acta tui                   # interactive picker (j/k/gg/G, Enter attaches)
```

How it works: `acta new` spawns a small daemon (`setsid`, no controlling
terminal) that owns the agent's PTY, appends all output to
`~/.acta/sessions/<id>.log`, and serves attach clients over a unix socket
with a 256 KiB scrollback replay. Detaching, closing your terminal, or
losing SSH never touches the agent.

Inside a session the agent sees `ACTA_SESSION` and `ACTA_SESSION_NAME`, so
anything it pushes to the clipboard queue is attributed to it.

## Acta Clipboard (`acta cb`)

The agent-to-human handover queue. `acta clipboard` is canonical; `acta cb`
is the alias — identical subcommands:

| Command | Alias | What it does |
| :--- | :--- | :--- |
| `acta clipboard push [content]` | `acta cb p` | Queue a snippet (or pipe via stdin). `--desc`, `--sensitive` |
| `acta clipboard next` | `acta cb n` | Pop the head onto your system clipboard |
| `acta clipboard list` | `acta cb ls` | Show the queue (sensitive content is masked) |
| `acta clipboard peek` | | Show the head without popping |
| `acta clipboard skip` | | Drop the head without copying |
| `acta clipboard clear` | `acta cb cl` | Empty the queue; `--sensitive` also wipes the system clipboard |

The workflow that kills heredoc hell: an agent pushes five setup commands,
then you just `Cmd+V`, `acta cb n`, `Cmd+V`, `acta cb n`, … until the queue
is empty.

Clipboard writes try `pbcopy`/`wl-copy`/`xclip`/`xsel` first, then fall back
to an **OSC 52** escape — so `acta cb next` reaches your local clipboard
even over a plain SSH connection (iTerm2, WezTerm, kitty, Ghostty, tmux, …).
Headless? `acta cb next --stdout` prints instead.

## Acta Workspaces (`acta ws`)

Working across many repos with submodules or worktrees is miserable. An
Acta workspace is one orchestrator repo that holds your shared agent config
and skills, plus an `acta.yaml` manifest. Every listed repo is cloned into a
gitignored `clones/` directory — real, independent clones, so agents can
work all of them concurrently with no worktree locking or branch juggling.

```bash
acta ws init my-stack                       # acta.yaml + gitignored clones/
acta ws add git@github.com:me/api.git
acta ws add git@github.com:me/web.git --branch develop
acta ws sync                                # parallel clone/pull + links + setup
acta ws status                              # branch / dirty / ahead-behind per clone
acta ws run -- git fetch --all              # run a command in every clone
acta new claude --repo api                  # launch an agent inside a clone
```

`acta.yaml`:

```yaml
workspace:
  name: my-stack
  clones_dir: clones
  # Shared files symlinked into every clone (skills, agent config).
  # They're auto-added to each clone's .git/info/exclude so they never
  # show up as untracked noise.
  links:
    - CLAUDE.md
    - .claude
repos:
  - name: api
    url: git@github.com:me/api.git
    setup:
      - make deps
  - name: web
    url: git@github.com:me/web.git
    branch: develop
```

### LLM context from the whole workspace

```bash
acta ws context --diff          # <acta-context clone="..."> blocks per repo
acta ws context --diff --clip   # …copied to your clipboard + queued in acta cb
```

Emits status and diffs for every clone wrapped in semantic tags, ready to
paste into any agent prompt:

```
<acta-context clone="api">
# git status --short --branch
## main...origin/main
 M src/server.rs
# git diff (staged + unstaged)
…
</acta-context>
```

## Agents & plugins

`claude`, `opencode`, `cursor`, `codex`, `gemini`, and `aider` work out of
the box; anything else runs as a raw command (`acta new ./my-agent.sh`).
Register your own:

```bash
acta plugin register myagent "myagent-cli"
acta plugin list
```

Configuration lives in `~/.config/acta/config.yaml`:

```yaml
plugins:
  claude:
    command: claude
    args: []
    env:
      ANTHROPIC_API_KEY: "${ANTHROPIC_API_KEY}"   # ${VAR} expands at launch
```

`ACTA_ENV` is surfaced in `acta ls`/`acta new` output and passed through to
agents, so prompts and hooks can behave differently in `prod` vs `dev`.

## Architecture

- **CLI** — clap command tree; `cb`/`ws` aliases; hidden `__sessiond` daemon entry
- **Session daemon** — portable-pty + tokio unix sockets, framed protocol,
  scrollback replay, setsid detachment
- **Clipboard** — flock-guarded JSON queue in `~/.acta/`, OSC 52 fallback
- **Workspace** — `acta.yaml` manifest, parallel git clone/pull via the
  system git (inherits your SSH agent / credential helpers)
- **TUI** — ratatui session picker with vim motions

## Roadmap

- [ ] `acta cb` daemon with UDS push from remote runners (clipboard over the network)
- [ ] Security harness: `ACTA_ENV=prod` command blocking + audit log
- [ ] Terminal-driver plugins (tmux, WezTerm, zellij panes as session frontends)
- [ ] `acta browser` / `acta mac` — the Agent-to-OS layer
- [ ] Session snapshot/restore

## Development

```bash
cargo build
cargo test
./target/debug/acta new bash --name demo -- -c 'while true; do date; sleep 1; done'
./target/debug/acta attach demo   # Ctrl-\ to detach
```

## License

MIT — see [LICENSE](./LICENSE).

---

> **Acta, non verba.** *Deeds, not words.*

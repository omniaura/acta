# Acta

**A terminal multiplexer for agentic coding** — like tmux, but for AI agents.

```bash
acta new claude      # Spin up Claude Code in an isolated worktree
acta new opencode    # Spin up OpenCode in a parallel worktree
acta ls              # List active sessions
acta attach fix-api  # Reattach to a running session
acta kill fix-api    # Terminate a session
```

## Overview

Acta provides session isolation and orchestration for AI coding agents. Each session runs a different agent (Claude Code, OpenCode, Cursor) in its own **Git worktree**, with PTY-based terminal multiplexing for attach/detach.

**Key features:**
- **Git worktree isolation** — each agent session gets its own worktree, so agents don't interfere with each other
- **PTY multiplexing** — attach/detach from running agents like tmux (Ctrl+B d to detach)
- **Per-session daemons** — each session runs independently, no single point of failure
- **Plugin system** — register any CLI tool as an agent
- **TUI dashboard** — visual session management with live status
- **Auto-cleanup** — worktrees, sockets, and metadata cleaned up on kill

## Installation

### From Source

```bash
git clone https://github.com/omniaura/acta
cd acta
cargo build --release
cargo install --path .
```

**Requirements:** Rust 1.75+, Git 2.20+

## Quick Start

```bash
# Must be inside a git repository
cd my-project

# Create a new Claude Code session
acta new claude

# Create a named session
acta new claude --name fix-auth

# Create a session without attaching (background)
acta new opencode --name api-refactor -d

# List active sessions
acta ls

# Attach to a running session
acta attach fix-auth

# Detach: press Ctrl+B then d

# Kill a session (preserves worktree)
acta kill fix-auth

# Kill and remove worktree
acta kill fix-auth --clean
```

## How It Works

### Architecture

```
acta new claude
  1. git worktree add .acta/worktrees/<session>
  2. Spawn daemon process (acta daemon --session-id <id>)
  3. Daemon creates PTY pair + spawns agent in worktree
  4. Client connects via Unix socket + bridges terminal I/O

acta attach <id>
  Connect to daemon's Unix socket, enter raw terminal mode

acta kill <id>
  SIGTERM to daemon -> kills agent -> cleanup
```

Each session is managed by an independent **daemon process** that:
- Creates a PTY pair (master/slave) via `openpty()`
- Spawns the agent process with the PTY slave as its terminal
- Listens on a Unix domain socket (`~/.acta/sessions/<id>.sock`)
- Bridges I/O between connected clients and the PTY master
- Handles agent exit and cleanup

### File Layout

```
~/.acta/sessions/           # Session metadata + sockets
  fix-auth.json             # Session state (agent, PID, paths)
  fix-auth.sock             # Unix domain socket (when running)
  fix-auth.log              # Daemon log output

<repo>/.acta/worktrees/     # Git worktrees (gitignored)
  fix-auth/                 # Isolated working copy
```

## Commands

### Session Management

- `acta new <agent>` — Create new agent session (auto-attaches)
- `acta new <agent> -d` — Create session in background
- `acta new <agent> --name <n>` — Create named session
- `acta ls` / `acta list` — List all sessions with live status
- `acta attach <id>` — Attach to running session
- `acta kill <id>` — Kill session (preserve worktree)
- `acta kill <id> --clean` — Kill session and remove worktree
- `acta tui` — Open interactive TUI dashboard

### Configuration

- `acta config list` — Show configuration
- `acta config set <key> <val>` — Set config value
- `acta config path` — Show config file location

### Plugins

- `acta plugin list` — List registered agents
- `acta plugin register <name> <cmd>` — Register new agent
- `acta plugin remove <name>` — Remove agent

## Configuration

Configuration is stored in `~/.config/acta/config.yaml`:

```yaml
plugins:
  claude:
    command: "claude"
    args: []
    env:
      ANTHROPIC_API_KEY: "${ANTHROPIC_API_KEY}"

  opencode:
    command: "opencode"
    args: []
    env: {}
```

Register any CLI tool as an agent:

```bash
acta plugin register aider "aider"
acta new aider --name refactor-db
```

## TUI Dashboard

Launch the interactive TUI with `acta tui`:

- **j/k** — Navigate sessions
- **Enter** — Attach to selected session
- **r** — Refresh session list
- **q** — Quit TUI

Sessions auto-refresh every 2 seconds with live PID/status tracking.

## Detach Key

While attached to a session, press **Ctrl+B** then **d** to detach. The agent continues running in the background.

## Development

```bash
cargo build
RUST_LOG=acta=debug cargo run -- new bash --name test -d
```

### Stack

- **Rust** with Tokio async runtime
- **libc** for PTY operations (openpty, ioctl, setsid)
- **Unix domain sockets** for client-daemon communication
- **clap** for CLI, **ratatui** for TUI
- **serde_json** for session state persistence

## Roadmap

### Phase 1: MVP (done)
- [x] CLI framework with all session commands
- [x] Git worktree isolation per session
- [x] PTY-based session daemons with Unix socket IPC
- [x] Attach/detach (Ctrl+B d)
- [x] Session lifecycle management
- [x] Plugin system for agent registration
- [x] TUI dashboard with live status

### Phase 2: Polish
- [ ] SIGWINCH forwarding (terminal resize)
- [ ] Scrollback buffer on reattach
- [ ] Session logs viewer
- [ ] Multi-pane TUI layout

### Phase 3: Integration
- [ ] AgentFlow integration
- [ ] Remote session support
- [ ] Agent output streaming/tailing
- [ ] Diff viewer for worktree changes

## License

MIT License - see LICENSE file for details.

---

*"Acta" — Latin for "acts" or "things done"*

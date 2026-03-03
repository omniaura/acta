# Acta

**A terminal multiplexer for agentic coding** — like tmux, but for AI agents.

```bash
acta new claude      # Spin up Claude Code in new worktree
acta new opencode    # Spin up OpenCode in parallel worktree
acta ls              # List active sessions
acta attach 3        # Attach to session #3
acta kill 2          # Terminate session #2
```

## Overview

Acta provides session isolation and orchestration for AI coding agents. Each "pane" runs a different agent (Claude Code, OpenCode, Cursor) in its own Git worktree, with ephemeral sessions that clean up after themselves.

**Status:** 🚧 MVP sprint in progress (v0.1.0)

## Features (Planned)

- ✅ CLI framework with Clap
- ✅ Async runtime with Tokio
- ✅ Git worktree isolation
- ✅ Session management
- ⏳ Plugin system for agents
- ⏳ Ratatui TUI interface
- ⏳ Configuration management

## Installation

### From Source

```bash
git clone https://github.com/omniaura/acta
cd acta
cargo build --release
cargo install --path .
```

## Quick Start

```bash
# Create a new Claude Code session (creates isolated git worktree)
acta new claude

# Create a session with a custom name
acta new opencode --name my-feature

# List active sessions
acta list

# Attach to a session (launches configured plugin in that worktree)
acta attach <session-id>

# Kill a session
acta kill <session-id>
```

## Commands

### Session Management

- `acta new <agent>` — Create new agent session
- `acta list` (`acta ls`) — List active sessions
- `acta attach <session>` — Attach to a session
- `acta detach` — Detach from current session
- `acta kill <session>` — Terminate a session

### Configuration

- `acta config list` — Show configuration
- `acta config get <key>` — Get config value
- `acta config set <key> <value>` — Set config value
- `acta config path` — Show config file location

### Plugins

- `acta plugin list` — List available plugins
- `acta plugin register <name> <command>` — Register plugin
- `acta plugin remove <name>` — Remove plugin

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
    args: ["--experimental"]
    env: {}
```

## Architecture

- **CLI** — Clap-based command parser
- **Session** — Session lifecycle management
- **Git** — Worktree isolation (planned)
- **TUI** — Ratatui interface (planned)
- **Config** — YAML configuration with Viper-like overlays

## Development

### Prerequisites

- Rust 1.93+ (edition 2021)
- Git 2.40+

### Build

```bash
cargo build
```

### Test

```bash
./target/debug/acta --help
./target/debug/acta new claude
./target/debug/acta list
```

### Dependencies

- **clap** — CLI framework
- **ratatui** — TUI framework
- **tokio** — Async runtime
- **serde** — Configuration serialization
- **anyhow/thiserror** — Error handling
- **tracing** — Structured logging

## Roadmap

### Phase 1: MVP (Current)
- [x] Basic CLI structure
- [x] Command parsing
- [x] Session state management
- [x] Git worktree operations
- [ ] Basic TUI

### Phase 2: Core Features
- [ ] Multi-pane layout
- [ ] Session persistence
- [ ] Plugin system
- [ ] Advanced worktree management

### Phase 3: Integration
- [ ] AgentFlow integration
- [ ] Mac Runner support
- [ ] Remote session support
- [ ] Cloud agent orchestration

## Contributing

Acta is part of the [OmniAura](https://github.com/omniaura) ecosystem. Contributions welcome!

## License

MIT License - see LICENSE file for details

## Links

- **Repository:** https://github.com/omniaura/acta
- **OmniAura:** https://github.com/omniaura
- **Spec:** [acta-spec.md](/workspace/group/acta-spec.md)

---

*"Acta" — Latin for "acts" or "things done"*

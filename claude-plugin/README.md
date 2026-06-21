# Vibird — Claude Code plugin

This plugin wires [Vibird](https://github.com/Hakureirm/vibird) into Claude Code:

- **Hooks** (`hooks/hooks.json`) push Claude's live state to the device on every turn — they run
  `vibird hook <event>`, which forwards the mapped state to the bridge's local control plane.
- **MCP** (`.mcp.json`) exposes device tools to the agent via `vibird mcp`.
- **Skill** (`skills/vibird-setup/`) teaches the agent to set the device up for the user (zero-config).

## Prerequisites

The `vibird` CLI must be on `PATH` and the bridge running:

```bash
cargo install vibird          # or: pip install vibird
vibird serve --tmux claude    # start the bridge (point --tmux at your Claude Code tmux target)
```

## Install

Until this ships to a plugin marketplace, install from the repo (the CLI can do it for you):

```bash
vibird claude install         # copies this plugin into ~/.claude and enables it
```

…or add the repo as a plugin marketplace in Claude Code and enable **vibird** from `/plugin`.

## What you'll see

The device shows the **Liz「栗子」** character; her face follows the agent state — idle when waiting,
thinking when you submit a prompt, working during tool calls, attention when Claude needs you, done at the
end of a turn. Hold the device button to talk; your words are transcribed and typed into the session.

License: AGPL-3.0-or-later + commercial (see the repo `LICENSE`).

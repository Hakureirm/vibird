---
name: vibird-setup
description: Set up and control a Vibird voice + status device — start the bridge, pair the device over WiFi, and verify the agent-state face (the Liz character). Use when the user mentions Vibird, "my device", the desk companion, voice input, or wants the device to mirror Claude's status.
---

# Vibird setup

Vibird is a small desk device whose screen shows a cute character (**Liz「栗子」**) that mirrors Claude
Code's live state — idle / listening / thinking / working / awaiting-approval / done — and lets the user
**hold-to-talk** to dictate intent to you. This skill is the "zero-config soul": **you** set it up for the
user instead of making them read a manual.

## How the pieces fit

```
device (Liz face) ──WiFi/WebSocket──▶ vibird bridge (local) ──┬─ ASR → tmux-inject your input
                  ◀── agent state ────────────────────────────┴─ control plane ◀─ this plugin's hooks
```

- This plugin's **hooks** already call `vibird hook <event>` on every turn — they push your state to the
  device automatically. **You never call `vibird hook` yourself.**
- This plugin's **MCP server** (`vibird mcp`) gives you tools to nudge the device directly (e.g. show a
  notification) when useful.

## First-time setup (do this when the user asks to set up Vibird)

1. **Check the CLI is installed:** run `vibird --version`. If missing, tell the user to
   `cargo install vibird` (or `pip install vibird`).
2. **Start the bridge** in the background, pointed at this session's tmux target so dictation lands here:
   `vibird serve --tmux "$(tmux display-message -p '#S:#I.#P' 2>/dev/null || echo claude)"`.
   It listens on `ws://0.0.0.0:8137` (devices) and a local control plane on `8138` (this plugin).
3. **Pair the device** (first time only, over USB serial): `vibird config` — it walks WiFi + bridge setup.
   After that the device auto-discovers the bridge via mDNS (`_vibird._tcp`).
4. **Verify:** the device should show Liz's idle face; run any tool and watch it switch to "working", then
   "done" when the turn ends.

## Voice input

The user holds the device button to talk; the bridge transcribes (configure real ASR with
`--asr cloud` + the `VIBIRD_ASR_*` env vars) and types the text into this session. Treat that text as the
user's prompt.

## Safety

The bridge runs locally; don't send device audio or transcripts anywhere except the user's configured ASR.
Keep any device-facing text SFW and friendly — Liz is a companion.

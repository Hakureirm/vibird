---
doc_kind: agent-index
last_verified_commit: eb40d35
---

# Vibird — Agent documentation line

Dense, English-canonical, versioned project memory for AI agents resuming context post-compaction. Humans
want the bilingual narrative in [`../human/`](../human/) instead.

## Start here (in order)

1. [SNAPSHOT.md](SNAPSHOT.md) — canonical current state (wedge, repo state, locked decisions, ADR roster,
   findings ledger, roadmap, open items). **READ FIRST.**
2. [adr/](adr/) — Architecture Decision Records (why things are the way they are).
3. [findings/](findings/) — observations + evidence (bugs, spikes, measurements).
4. [atoms3r-hardware.md](atoms3r-hardware.md) — source-verified AtomS3R pinout / reference.

## Conventions (ADSD)

- Every cross-file decision → an ADR. Every observation → a finding. Negative results are first-class.
- SNAPSHOT is the source of truth; `README.md` + `../human/` are projections.
- `last_verified_commit` frontmatter on every stateful doc.
- Atomic commits (code + tests + docs together). Commit messages in English (international OSS).

## Build / flash (from repo root)

- host:     `cd host && cargo run -- serve`
- firmware: `. ~/export-esp.sh && cd firmware && cargo run --release`
- network:  keep `all_proxy` / `http_proxy` **unset** (router transparent proxy).

## Code map

- `../../firmware` — Rust (esp-hal, no_std) device client + (planned) emote region-flush player.
- `../../host`     — Rust workspace: bridge / cli / (planned: `vibird-emote-pack`, ASR, MCP).
- `../../protocol` — `vibird-protocol` shared types + `PROTOCOL.md`.

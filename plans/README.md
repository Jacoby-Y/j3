# Plans

Handoff plans for open work. Finished plans are deleted; git history keeps them. Code comments that
say "plan 023" and the like point at those old, deleted plans.

| Plan | What | Status |
|------|------|--------|
| [01](01-steam.md) | Steam: what to verify with the real SDK, client, Windows, Deck and sniper | TODO (needs the developer) |
| [version1](version1.md) | Public release readiness audit (2026-10-29): what blocks a 1.0 | TODO |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with one-line reason). A new plan takes the next
number. Open direction lives in `roadmap.md` and `EDITOR.md`.

`cargo build` and `cargo test` in `j3c/` run `.cargo/copy-j3c.sh`, which moves the new binary to
`~/.bin/j3c`; set `J3C_NO_INSTALL=1` while working and run `./build.sh` when done.

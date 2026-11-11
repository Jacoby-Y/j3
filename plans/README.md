# Plans

Handoff plans for open work. Finished plans are deleted; git history keeps them. Code comments that
say "plan 023" and the like point at those old, deleted plans.

| Plan | What | Status |
|------|------|--------|
| [01](01-steam.md) | Steam: what to verify with the real SDK, client, Windows, Deck and sniper | TODO (needs the developer) |
| [02](02-public-release.md) | Public release checklist: clone-ability, license, hygiene, README, installers, CI | IN PROGRESS |
| [03](03-ui-cpu.md) | `.j3ui` CPU per frame: layout boundaries, retained paint, cheaper restyles (stress scene) | TODO |
| [version1](version1.md) | Public release readiness audit (2026-10-29): what blocks a 1.0 | TODO |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with one-line reason). A new plan takes the next
number. Open direction lives in `roadmap.md` and `EDITOR.md`.

Plain `cargo build` in `j3c/` installs nothing. A developer can opt in to `j3c/.cargo/copy-j3c.sh` (a
rustc wrapper that copies each new binary to `$J3_BIN_DIR`, default `~/.bin`) from an untracked cargo
config; with it on, set `J3C_NO_INSTALL=1` while working and run `./build.sh` when done.

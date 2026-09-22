# Plan 007: Roadmap and AGENTS match what already ships

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Plan 006 must already
> be DONE (the opening lines of `roadmap.md` and `AGENTS.md` no longer call
> `speculation.md` the law). Confirm the code excerpts below. On a mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: plans/006-retarget-language-contract.md
- **Category**: docs
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`roadmap.md` still lists inclusive `for`, default fields, and `@j3ui` as work
to do, and still says a `for` index lives until the end of the function.
Those four are in the compiler. Root `AGENTS.md` and `j3c guide` omit
`speculative/10`, `examples/defense`, and `examples/gallery`. Agents re-open
finished work and miss the examples that are actually being edited.

## Current state

Already shipped, with evidence:

- Inclusive ranges: `j3c/src/parse/parser.rs` treats `TokenKind::DotDotEq` as inclusive (around line 990). Test `inclusive_for_runs` in `compile.rs` (around line 860) expects stdout `0123\n`.
- Default fields: test `default_fields_fill_omitted` (around line 909) accepts `struct Hit { ok: bool = false; x: int = 0; }` and `Hit {}`.
- `for` index scope: `j3c/src/check/lower.rs` after pushing `ForRange` does `self.locals.remove(&s.name.name)` (around line 621). Test `for_index_ends_with_the_loop` (around line 875) typechecks two `for i` loops in one function. `let_then_for_still_shadows` still errors.
- `@j3ui`: `j3c/src/parse/parser.rs` `j3ui_item` (around line 467). Examples: `examples/defense/src/hud.j3`, `examples/gallery/src/`. Acceptance file `speculative/10_j3ui.j3`. `j3c/src/compile.rs` has `defense_project_typechecks` and a gallery project test.

Still true, leave them as open language friction:

- Left-hand literals. `j3c/stdlib/j3/math.j3` `lookAt` still has `f32(0) - f.x` (around lines 380–388). Do not claim this shipped.
- `Hit`, `Texture`, and `Mesh` still have no defaults. Plan 012 adds them. This plan must not say those structs already use defaults. It may say the language feature exists and the stdlib structs do not use it yet.

`roadmap.md` "Language features" items 2, 3, and 4, and the whole `## @jui` "Held" section, are the stale parts. Item 1 (left-hand literals) stays.

`AGENTS.md` table row for `speculative/` says `01`–`09` only. There is no row for defense or gallery. `j3c/src/helptext.rs` `GUIDE` lists bounce, dungeon, island, minecraft, leaves (around lines 41–46) and not defense or gallery.

`j3c/AGENTS.md` says hello + `01`–`08` compile and run, and `09` typechecks. Add `10` only if a `compile.rs` test already runs or typechecks `10_j3ui.j3`. Search first. If no test runs `10`, say `speculative/10_j3ui.j3` exists and defense/gallery typecheck; do not claim `10` is an acceptance run.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Guide still builds | `cd j3c && cargo test --lib helptext -- --test-threads=1` | exit 0, or no test named helptext and `cargo test hello_j3_prints_hello_world` exits 0 |
| Stale phrases | `rg -n "Held\\.|No defaults and no positional|for inserts the index and never removes|01\`–\`09" roadmap.md AGENTS.md j3c/AGENTS.md j3c/src/helptext.rs` | no matches |

The `01`–`09` pattern in the rg command is the AGENTS table phrase. After the edit that row must mention `10`.

## Scope

**In scope**:
- `roadmap.md` (Language features list and `@jui` section; one line in Shipped if that section exists)
- `AGENTS.md` (the path table only)
- `j3c/AGENTS.md` (the sentence that lists which speculative programs run, only)
- `j3c/src/helptext.rs` (`GUIDE` example list only)

**Out of scope**:
- Implementing language features
- Plan 012's struct edits
- `editors/vscode/`
- Rewriting non-goals

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Roadmap

In `## Language features`, delete items 2 (inclusive `for`), 3 (default fields), and 4 (`for` index scope) from the "to do" list. Add a short "Shipped since this list was written" paragraph that names those three and points at the tests above. State that `let i` then `for i` is still a shadowing error.

Keep item 1 (left-hand literals) as the remaining language item. Drop the sentence "First slice: left-hand literals, `..=`, default fields."

Replace `## @jui` "Held" / "Wait until button can refuse..." with: `@j3ui` is in the parser; defense and gallery call it; `button` still has no disabled/refuse API (`j3c/stdlib/j3/ui.j3` `button`, around lines 512–552). Do not paste a new language design. Delete the large sample that shows the future syntax if it is presented as not-yet-implemented. If you are unsure whether a paragraph describes shipped behavior, STOP.

### Step 2: AGENTS and guide

`AGENTS.md` speculative row: `01`–`08` compile, `09` typechecks `j3.game`, `10_j3ui.j3` is the `@j3ui` sample. Add table rows for `examples/defense/` and `examples/gallery/` in the same style as the leaves row (one sentence from the `j3.toml` / `src/` you actually open: defense is a project with `src/world.j3` and `src/hud.j3`; gallery is a UI project with `src/`). Do not invent SDL requirements you did not see in those `j3.toml` files. Open `examples/defense/j3.toml` and `examples/gallery/j3.toml` and mention link libs only if they are listed there.

`helptext.rs` `GUIDE`: add defense and gallery to the example list, one line each, same column style as leaves.

`j3c/AGENTS.md`: extend the acceptance sentence. Do not claim `10` runs unless `rg "10_j3ui" j3c/src/compile.rs` finds a test that compiles it.

### Step 3: Check

**Verify**: the `rg` in the commands table prints nothing.
**Verify**: `cd j3c && cargo test hello_j3_prints_hello_world -- --test-threads=1` → exit 0.

## Test plan

- Docs only, plus a compile of `helptext.rs` via `cargo test`.
- No new test.

## Done criteria

- [ ] `roadmap.md` does not say inclusive `for`, default fields, or `for`-index scope are unimplemented
- [ ] `roadmap.md` still lists left-hand literals as open
- [ ] `AGENTS.md` mentions defense, gallery, and `speculative/10`
- [ ] `GUIDE` in `helptext.rs` mentions defense and gallery
- [ ] `cargo test hello_j3_prints_hello_world` exits 0
- [ ] Plan 007 row is DONE

## STOP conditions

- Plan 006 is not DONE (files still say `speculation.md` is the law). Finish 006 first.
- `for_index_ends_with_the_loop` is gone or fails. The scope claim would be false. STOP.
- You cannot tell whether an `@jui` paragraph is shipped or aspirational. STOP rather than deleting the design notes blindly. Report which paragraphs you left.

## Maintenance notes

- When plan 012 lands, one sentence in the roadmap can say `Hit` / `Texture` / `Mesh` use defaults. Do not add that sentence in this plan.
- Left-hand literals stay open until a later plan. Do not start that work here.

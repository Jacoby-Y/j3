# Plan 006: Stop citing a speculation.md that is not in the tree

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm each citation
> below still exists. On a mismatch, STOP. Do not invent a language spec.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: MED
- **Depends on**: none
- **Category**: docs
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

Root `AGENTS.md`, `j3c/AGENTS.md`, and `roadmap.md` say `speculation.md` wins
when it disagrees with the compiler. That file is not in the tree, and there
is no git history to restore it. Agents are told to "fix the crate" to match
a document they cannot read. This plan points those sentences at the files
that exist. It does not write a replacement spec.

## Current state

Citations found by search (update every one; do not add new section numbers):

- `AGENTS.md` line 7: table row `` `speculation.md` | Language + compiler contract ``
- `roadmap.md` line 3: `Spec (`speculation.md`) wins.` then the non-goals sentence
- `j3c/AGENTS.md` line 3: `Language law is `../speculation.md`.` and the next sentence that the doc wins over the crate
- `j3c/AGENTS.md` around line 153: `Comments point at `speculation.md` sections, not at novels.`
- `j3c/src/lib.rs` line 3: `Pipeline (speculation.md §16): parse → typecheck → typed IR → emit (LLVM default, C optional).`
- `j3c/src/ir/mod.rs` line 6: `Places are first-class (speculation.md §2).`
- `j3c/src/parse/syntax.rs` line 27: `Builtin type spellings (speculation.md §4.1). Checker must accept every name.`
- `j3c/examples/defer_scope.j3` line 1: `// Locks block-scoped defer (speculation.md). Not function-scoped.`

Living contract that already exists:

- `j3c/AGENTS.md` — pipeline, IR rules, non-goals ("Do not build")
- `roadmap.md` — non-goals in the first paragraph after the spec sentence
- `speculative/01`–`08` — programs that compile and run; `09` typechecks

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| No remaining cites | `rg -n "speculation.md" --glob '!plans/**' --glob '!j3c/target/**'` from the repo root | no matches |
| Comment-only rust | `cd j3c && cargo test --lib compile::tests::hello_j3_prints_hello_world -- --test-threads=1` | exit 0 |

`rg` if missing: `grep -RIn "speculation.md" --exclude-dir=target --exclude-dir=plans .`

## Scope

**In scope**: the eight locations listed above.

**Out of scope**:
- Writing `speculation.md`
- Rewriting the language-features or `@jui` sections of `roadmap.md` (plan 007)
- Changing compiler behavior
- `j3c/target/`

## Git workflow

No git repository. Do not commit. Do not create `speculation.md`.

## Steps

### Step 1: Retarget the three docs

`AGENTS.md` table: replace the `speculation.md` row with:

`j3c/AGENTS.md` | Language + compiler contract (there is no `speculation.md`)

`roadmap.md` first sentence: replace `Spec (`speculation.md`) wins.` with
`Non-goals (no separate spec file in this tree):` and keep the rest of that
paragraph (no GC, no borrow checker, and the "Do not ..." sentences) unchanged.

`j3c/AGENTS.md` opening: replace the law sentence with: the contract is this
file, `roadmap.md`'s non-goals, and `speculative/` (01–08 run, 09 typechecks).
If this file and those disagree with an example, the example's test in
`compile.rs` is what `cargo test` locks. Delete the sentence that says a
missing doc wins over the crate.

Style bullet around line 153: `Comments name the rule. Do not cite speculation.md.`

### Step 2: Retarget the four code comments

- `lib.rs`: `Pipeline: parse → typecheck → typed IR → emit (LLVM default, C optional).`
- `ir/mod.rs`: keep the places sentence, drop `(speculation.md §2)`.
- `syntax.rs`: `Builtin type spellings. Checker must accept every name.`
- `defer_scope.j3`: `// Locks block-scoped defer. Not function-scoped.`

### Step 3: Search

**Verify**: from the repo root, `rg -n "speculation.md" --glob '!plans/**' --glob '!j3c/target/**'` prints nothing.
**Verify**: `cd j3c && cargo test --lib hello_j3_prints_hello_world -- --test-threads=1` → exit 0.
If `--lib` does not select that test, run `cargo test hello_j3_prints_hello_world -- --test-threads=1`.

## Test plan

- No new test. The comment edits must not change behavior. One existing hello test is the compile check.

## Done criteria

- [ ] Repo search for `speculation.md` outside `plans/` and `j3c/target/` is empty
- [ ] `speculation.md` was not created
- [ ] `cargo test hello_j3_prints_hello_world` exits 0
- [ ] Plan 006 row is DONE
- [ ] Language-features and `@jui` sections of `roadmap.md` are untouched (plan 007)

## STOP conditions

- `speculation.md` exists when you start. STOP. Do not delete or rewrite it.
- A citation sits in a file not listed here. Add it to the same retarget (one sentence, no new spec) only if the file is markdown, a rust comment, or a `.j3` comment. STOP if it is in executable logic.
- Plan 007 has already edited `roadmap.md` / `AGENTS.md` and your excerpts do not match. Read plan 007 and apply only the citation change, or STOP if the files already say there is no speculation.md.

## Maintenance notes

- If a real spec is written later, put it at `speculation.md` and point `AGENTS.md` back at it in one edit. Do not leave both "no spec" and a spec file.
- Plan 007 runs after this and will edit the same three docs. Its job is the shipped-feature list, not this citation.

# Plan 008: One LSP check per buffer version

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm the call sites
> below. On a mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

Every `textDocument/didChange` runs a full parse and check to publish
diagnostics. Hover, definition, signature help, and completion each run that
check again on the same buffers. `j3.game` is about 2460 lines and `j3.gfx`
about 2040. The cache is per buffer version: a keystroke still rechecks once,
and the following hover or completion reuses that result. It does not skip
the check on the edit itself.

## Current state

`j3c/src/lsp/mod.rs` `serve()` holds `docs: HashMap<String, String>`.

- `didOpen` / `didChange` call `publish_diags` (around lines 105 and 121).
- `publish_diags` (around line 505) calls `compile::analyze_checked`.
- `pos_query` (around line 485) calls `query::analyze` for hover and definition.
- `program_for` (around line 416) calls `query::analyze` for signature help,
  and falls back to `query::analyze(path, &[])`.
- Completion (around line 173) calls `complete::complete_buffer`, which calls
  `query::analyze` up to three times (`j3c/src/lsp/complete.rs` lines 72–84):
  overlays, a healed blanked line, then disk.

`complete_at` is already `pub` (`complete.rs` line 57):

```rust
pub fn complete_at(prog: &Program, path: &str, src: &str, offset: u32) -> Vec<Completion>
```

`query::analyze` is `compile::analyze_path`. `Program` is `crate::check::Program`.
Do not assume it is `Clone`.

Existing tests in `j3c/src/lsp/query.rs` call `analyze` directly. Leave them.
They do not go through `serve()`.

Style: no crates.io, no macros, explicit matches. The LSP JSON parser is
`j3c/src/lsp/json.rs`. Do not replace it.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| LSP tests | `cargo test --lib lsp:: -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

If `--lib lsp::` matches nothing, run `cargo test hover_print_ln_on_hello -- --test-threads=1`.

## Scope

**In scope**:
- `j3c/src/lsp/mod.rs`
- `j3c/src/lsp/complete.rs` only if `complete_buffer` must accept an optional already-checked program. Prefer not to change it: the completion handler calls `complete_at` on a cache hit and `complete_buffer` only on a cache miss.

**Out of scope**:
- `j3c/src/check/`, `j3c/src/compile.rs` (except you must not change analyze semantics)
- Incremental / partial recheck of one function
- A cache that survives `didChange` (the text changed; the key must change)
- Rename, formatting, or new LSP methods

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Cache struct and key

In `lsp/mod.rs`:

```rust
struct ProgramCache {
    key: u64,
    prog: Option<crate::check::Program>,
}

fn docs_key(docs: &HashMap<String, String>) -> u64
```

`docs_key` sorts keys, then hashes each URI and text with
`std::collections::hash_map::DefaultHasher` and `std::hash::Hash`. Same map
contents → same `u64`. One changed character → a different `u64`.

```rust
impl ProgramCache {
    fn get<F>(&mut self, key: u64, load: F) -> Option<&crate::check::Program>
    where
        F: FnOnce() -> Option<crate::check::Program>,
    {
        if self.prog.is_none() || self.key != key {
            self.key = key;
            self.prog = load();
        }
        self.prog.as_ref()
    }
}
```

Initialize `key` to 0 and `prog` to `None` in `new()` or a struct literal in
`serve`. A failed `load` stores `None`, so the next call with the same key
tries again. That is correct: do not cache failures.

### Step 2: Unit-test the key and the hit

Add `#[cfg(test)] mod tests` in `lsp/mod.rs` if it has none.

- `docs_key` of two maps with the same pair is equal.
- Changing the string value changes the key.
- `ProgramCache::get` with a counter closure: first call and a second call
  with the same key invoke the closure once total. A third call with a new
  key invokes it again.

The closure can return `None` for the counter test if constructing a
`Program` in the unit test is heavy. Then also one test that `get` returns
`None` when `load` returns `None`, and a later `get` with the same key calls
`load` again. Use a real `Program` only if `check::check_files` on a tiny
`module t; fn main() {}` is easy from this module. If that import cycle is
messy, the `None`-load counter test is enough. Do not spin up stdio LSP.

### Step 3: Wire serve

Add `let mut cache = ProgramCache { key: 0, prog: None };` in `serve`.

`publish_diags` must fill the cache: after a successful `analyze_checked`,
store that program under `docs_key(docs)`. `analyze_checked` returns a
`Result` with a rich error; `query::analyze` returns `Result<Program, String>`.
Use one successful `query::analyze(path, &overlays_from(docs))` for the cache,
and keep the existing diagnostic path's error rendering. Do not analyze twice
on the success path: if you need the `CheckError` for diagnostics, call
`analyze_checked` once and on `Ok(prog)` store `prog` in the cache.

On `Err`, set `cache.prog = None` (and set `cache.key` to the new key) so
hover cannot show types from the previous version.

Change `pos_query` and `program_for` to take `&mut ProgramCache` (or take
`&Program` computed by the caller). Hover, definition, and signature help use
`cache.get(docs_key(docs), || query::analyze(...).ok())`.

Completion handler:

- On cache hit, `complete::complete_at(&prog, &cur.path_str, &cur.src, cur.offset)`.
- On cache miss, `complete::complete_buffer(...)` as today (healed line).
  Do not store the healed-line program in `cache`. That text is not the
  buffer.

`program_for`'s disk fallback (`analyze(path, &[])`) stays only when `docs`
has no overlay for that path. Do not let an empty-overlay success overwrite
a cache entry for a different key. Call it only when `cache.get` is `None`
and do not insert it under the docs key.

### Step 4: Tests

**Verify**: `cargo test --lib lsp:: -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- `docs_key` equality and inequality.
- `ProgramCache::get` calls `load` once for a repeated key and again after a key change.
- Failed `load` is not sticky.
- Existing `hover_print_ln_on_hello` still passes (it does not use the cache).

## Done criteria

- [ ] `publish_diags` and hover on an unchanged `docs` map do not both call `query::analyze` / `analyze_checked`. A code comment is not enough: the cache struct is used by both paths. Review by reading `serve`.
- [ ] `cargo test --lib lsp::` exits 0 (or the hover test plus the new cache tests)
- [ ] `cargo test` exits 0
- [ ] Healed completion does not write into the cache
- [ ] Plan 008 row is DONE

## STOP conditions

- `Program` cannot be stored because it borrows `docs`. It should own its strings. If the compiler says it contains a reference, STOP.
- Wiring the cache requires a second parser or a change to check. STOP.
- A test that talks to `serve()` over stdin looks necessary. STOP and report. The unit test of `ProgramCache` is the required test.

## Maintenance notes

- Any new LSP method that needs a `Program` must go through `ProgramCache::get` with `docs_key`.
- `didChange` updates `docs` before `publish_diags`. The key changes. Do not add a "same file, ignore text" shortcut.
- A partial recheck is out of scope. This cache is whole-program, one version.

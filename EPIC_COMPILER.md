# Compiler speed epic

Research notes from code inspection, 2026-10-08. These are proposed changes, not implemented features. No benchmarks were run.

## Done (2026-10-08)

Items 1, 3 and 5 are implemented. LLVM, C and IR output is byte-identical for 3 and 5 (fps-shooter, gallery, scripted, 11_script, blocks-adventure, editor_village).

- **5**: `SourceFile::line` binary-searches line starts; `IrCx::file` is a map lookup. fps-shooter opt 0 cold build 0.95 s to 0.69 s.
- **3**: generic origins and merged struct templates are built once per program, merged generic envs once per module, and the lower queue no longer clones bodies. `check: lower` 0.064 s to 0.035 s.
- **1**: opt-in `[emit] incremental = "all"`. fps-shooter at opt 2: cold 12.3 s to 2.1 s, one-function edit 12.2 s to 0.75 s.

## Baseline and estimates

The target is a very large J3 project with slow compilation at `opt=0` and much slower compilation at `opt=3`.

Verified current behavior:

- Normal LLVM builds at `opt=0` already cache module objects and compile cache misses in parallel (`j3c/src/emit/incr.rs`, `enabled` and `compile`).
- `opt=3` bypasses that path and emits the whole program for one Clang invocation (`j3c/src/emit/mod.rs`, `compile_program`).
- Parsing, checking, and lowering happen before the CLI's up-to-date check (`j3c/src/compile.rs`, `load_and_check_with`; `j3c/src/cli.rs`, `up_to_date`).
- Script development builds use a separate compilation path (`j3c/src/devrun.rs`, `prepare`).

All percentages below are rough guesses of **total compilation-time reduction**, relative to the current compiler in the stated situation. They are not measured results or runtime-performance estimates. A 20% time reduction means a build takes 80% of its previous time, equivalent to 1.25x compilation throughput. Estimates overlap and must not be added together. Cold means uncached; edit means rebuilding after a small source change.

The largest `opt=3` opportunity is avoiding whole-program reoptimization after every edit. At `opt=0`, the strongest opportunities are removing repeated frontend work and reducing what each module sends to Clang.

## 1. Cached, parallel module compilation at opt=3

**Evidence:** `j3c/src/emit/incr.rs`, `enabled`, explicitly restricts incremental compilation to `opt=0`. The module emitter, object cache, and worker queue already exist.

**Change and mechanism:** Allow an explicit optimized incremental build mode. Optimize unchanged modules once, reuse their objects, and compile changed modules concurrently rather than reoptimizing the entire program.

**Estimated improvement:** `opt=3`: **30–60% shorter cold builds**, **60–90% shorter small-edit builds**, with many reasonably balanced modules and enough cores/RAM. Normal `opt=0`: **approximately 0% additional gain**, since this already exists.

**Tradeoff:** Separate native objects lose cross-module inlining and other optimizations, which can affect runtime performance. Keep this an explicit build mode. This is an alternative to item 2; their estimates are not additive.

## 2. Cached ThinLTO for optimized builds

**Evidence:** The current optimized path emits one whole-program LLVM module. [LLVM's ThinLTO documentation](https://clang.llvm.org/docs/ThinLTO.html) describes parallel optimization backends, cross-module function importing, and persistent incremental caching.

**Change and mechanism:** Emit separate bitcode modules, then use ThinLTO with a supported linker and persistent cache. Reuse unchanged backend results while retaining cross-module optimization. Adding a flag to today's single giant module would not deliver the main benefit; compilation must be partitioned.

**Estimated improvement:** `opt=3`: **10–40% shorter cold builds**, **40–80% shorter small-edit builds**, when most modules and imported functions remain unchanged. `opt=0`: **approximately 0% benefit**.

**Tradeoff:** More implementation work and linker requirements than item 1. Changes to imported function bodies can invalidate otherwise unchanged consumers. Cold builds can benefit less or regress with unfavorable partitioning or memory pressure. Runtime equivalence and optimization quality remain unverified.

## 3. Shared generic environments instead of repeated cloning

**Evidence:** `j3c/src/check/lower.rs`, `generic_origins` and `lower_module`, repeatedly scans all modules, clones generic definition environments, gathers templates and instances, and copies AST bodies through multiple queues. `j3c/src/check/mod.rs`, `share_methods`, copies the complete method/template collection into every module.

**Change and mechanism:** Share immutable definition tables and keep mutable instance tables separate. Move or borrow queued AST bodies where ownership permits. Avoid rebuilding program-wide generic environments for each module, reducing repeated allocation and traversal.

**Estimated improvement:** `opt=0`: **10–30% shorter builds**; `opt=3`: **1–8% shorter builds**, especially with many modules and generics. The smaller `opt=3` percentage reflects its much larger backend cost.

**Tradeoff:** Preserve generic definition-site name resolution and existing instance ownership. Mutable tables need deliberate separation from shared definitions.

## 4. Emit only the declarations and types each module needs

**Evidence:** `j3c/src/emit/llvm.rs`, `emit_llvm_module`, emits program-wide types, function declarations, and globals into every uncached module. Function/global ownership checks use linear searches.

**Change and mechanism:** Collect actual foreign references and their required type closure. Emit only those declarations and types, and use indexed ownership checks. This shrinks emitter work and the LLVM input Clang must parse.

**Estimated improvement:** `opt=0`: **10–30% shorter cold builds**, **3–15% shorter small-edit builds**, particularly with hundreds of modules importing small portions of large libraries. Current whole-program `opt=3`: **0–3%**; this becomes more relevant after items 1 or 2.

**Tradeoff:** Include references from signatures, globals, function addresses, nested types, and compiler-generated entry/crash routines. Preserve required library link notes when reducing declarations.

## 5. Index source lines and files once

**Evidence:** `j3c/src/parse/mod.rs`, `line_of`, counts newlines from the beginning of the source on every call. `j3c/src/emit/llvm_expr.rs` calls it repeatedly, including without debug metadata because runtime errors need locations. `j3c/src/ir/cx.rs`, `file`, also searches a vector.

**Change and mechanism:** Precompute line-start offsets and index source files by `FileId`. Use binary search or a suitable position cache for line lookup instead of rescanning source prefixes.

**Estimated improvement:** `opt=0`: **5–20% shorter cold builds**; `opt=3`: **1–5% shorter builds**, chiefly for very large individual files. Small files or rebuilds hitting most object caches benefit much less.

**Tradeoff:** Preserve byte-offset semantics, dummy spans, end-of-file handling, and runtime-error line reporting. Line tables use modest additional memory.

## 6. Prune unreachable functions before backend emission

**Evidence:** `j3c/src/emit/llvm.rs`, `emit_llvm_part`, emits every checked function in loaded modules. There is no function-level pruning in that path.

**Change and mechanism:** Build a conservative reachability graph retaining entry points, referenced functions, global initializer dependencies, callbacks, and script registrations. Drop unreachable bodies before emission so Clang and the linker receive less work.

**Estimated improvement:** `opt=0`: **10–30% shorter cold builds**; `opt=3`: **10–35% shorter builds**, when large imported libraries contain substantial unused functionality.

**Tradeoff:** Function pointers and script/native registrations require conservative handling. Initially retain full checking so unused source still receives diagnostics; this saves emission/backend time rather than checking time. Gains are smaller when LLVM already eliminates unused bodies early.

## 7. Cache checked and lowered modules

**Evidence:** `j3c/src/compile.rs`, `load_and_check_with`, rebuilds the frontend program on every invocation. Object reuse happens after that work.

**Change and mechanism:** Add dependency-aware frontend caching. Reuse unchanged checked/lowered modules and invalidate consumers when interfaces, constants, generic bodies, or layouts change. Avoid parsing and lowering the same large stdlib and project modules on every edit.

**Estimated improvement:** Small-edit builds at `opt=0`: **25–60% shorter**, where object caching has already made frontend work dominant. Current `opt=3`: **2–10% shorter**, because whole-program optimization remains. Cold builds: **approximately 0%**, possibly slightly slower from cache bookkeeping.

**Tradeoff:** A substantial feature requiring correct dependency invalidation, stable identities or ID remapping, and bounded cache storage. Generic body dependencies matter even when signatures stay unchanged.

## 8. Detect unchanged builds before running the frontend

**Evidence:** `j3c/src/cli.rs`, `source_key` and `up_to_date`, operate on an already compiled `Program`.

**Change and mechanism:** Persist the previous build's dependency list and fingerprints. Check these before parsing/checking/lowering, allowing an unchanged build to return immediately.

**Estimated improvement:** **70–95% shorter unchanged build invocations at either optimization level**. Cold builds and actual source edits: **approximately 0% benefit**.

**Tradeoff:** Relevant manifests, scripts, scene-derived registry inputs, stdlib overrides, compiler options, and generated inputs must participate. Preserve necessary generator/preflight behavior and detect added or removed dependencies.

## 9. Use module caching for script development builds

**Evidence:** `j3c/src/devrun.rs`, `prepare`, separately caches the optimized VM below `opt=2`, but recompiles the remaining native program together. It bypasses the normal `compile_program` module-cache path.

**Change and mechanism:** Route native script-development compilation through module caching while preserving the optimized VM object and existing script-only reload behavior. Native edits then recompile only affected modules.

**Estimated improvement:** Script projects at `opt=0`: **30–70% shorter native-edit builds**, **10–30% shorter cold builds**. Script-only edits: **approximately 0% additional benefit**, since they already skip native compilation. `opt=3` requires item 1 or 2 for analogous gains.

**Tradeoff:** Preserve VM optimization, generated thunk registration, script layout handling, and native-symbol dependencies across cached objects.

## 10. Reduce object-cache bookkeeping

**Evidence:** `j3c/src/emit/incr.rs`, `object_key` and `module_ir`, build textual IR representations and scan layout-name tokens. `sweep_stale` recomputes keys. Stable symbols/type descriptions and ABI layouts are also repeatedly constructed or traversed in emission.

**Change and mechanism:** Reuse computed keys during cleanup, hash structured IR directly, and memoize stable symbols/type descriptions and finalized layouts. Reduce formatting, allocation, and repeated traversal on cache-hit-heavy builds.

**Estimated improvement:** `opt=0`: **3–12% shorter small-edit builds** with many cache hits. Current `opt=3`: **0–2%**. Most useful after larger frontend costs are reduced.

**Tradeoff:** Preserve all semantic, layout, debug, compiler, target, and option dependencies in cache keys. Memoize layouts only after the relevant type definitions are finalized.

## 11. Offer line-only debug metadata

**Evidence:** `j3c/src/emit/llvm_expr.rs`, `DebugOut::prepare` and `declare_locals`, construct full debug metadata and local-variable declarations.

**Change and mechanism:** Add a line-only debug mode retaining source traces while omitting detailed local-variable/type metadata. This reduces metadata generation and backend processing. J3 constructs LLVM metadata itself, so changing only the host `-g` flag is insufficient.

**Estimated improvement:** Debug-enabled builds: `opt=0` **5–15% shorter**, `opt=3` **3–12% shorter**, especially with many locals and aggregate types. Debug-disabled builds: **0% benefit**.

**Tradeoff:** Reduced variable inspection in debuggers. Preserve useful function/file/line information and crash symbolization.

## 12. Emit fewer temporary stack operations

**Evidence:** `j3c/src/emit/llvm_expr.rs`, `emit_func`, allocates every local on the stack. Compiler temporaries are identified by `LocalKind::Tmp` in the existing IR.

**Change and mechanism:** Emit straight-line, single-assignment compiler temporaries whose addresses never escape as LLVM values directly. Keep the existing place-based IR and avoid a CFG rewrite. Fewer allocations, loads, and stores mean less input and optimization cleanup for Clang.

**Estimated improvement:** `opt=0`: **3–10% shorter cold builds**; `opt=3`: **3–12% shorter builds**, in expression-heavy code generating many temporaries.

**Tradeoff:** More delicate than allocation and lookup fixes. Branches, mutation, address-taking, and debugging require careful handling; retain stack slots wherever eligibility is uncertain.

## Suggested order and remaining uncertainty

Start with **3, 5, and 4**, then **6**. For the much slower `opt=3` case, choose between **1 and 2** according to acceptable runtime-performance tradeoffs; frontend improvements alone are unlikely to remove that gap. Items **7 and 8** address repeated frontend work, with item 8 restricted to unchanged builds. Item **9** brings script-native edits into the existing incremental architecture.

Keep changes within the compiler contract: no new parser, crates.io dependencies, libLLVM, or conversion of checked IR to a CFG.

Validation of this research was limited to source inspection and official LLVM documentation. No benchmarks, builds, or tests were run. Actual phase costs, achievable percentages, cache correctness after implementation, and runtime-performance effects remain unverified.

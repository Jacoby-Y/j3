# Plan 018: J3s scripts (first-class, typed, hot-reloadable)

> **Executor instructions**: Follow this plan phase by phase. Run every
> verification command and confirm the expected result before moving on. If
> anything in "STOP conditions" occurs, stop and report; don't improvise.
> When done, do NOT update `plans/README.md` (the reviewer does that).
>
> **Drift check (run first)**: compare the "Current state" excerpts below to
> the live files. On a mismatch, STOP.

## Status

- **Status**: DONE (2026-09-26). Implementation notes and measurements at the end.
- **Priority**: P2
- **Effort**: XL (8 phases; A–C ship typed scripts with no VM, D–F add the VM and dev mode, G–H wire hosts)
- **Risk**: HIGH (new backend, new runtime, editor data format)
- **Depends on**: none hard. 008 (per-module objects) makes release builds faster. Replaces behaviors, so
  016 phase 3 ("Behavior catalog in the J3 editor") should wait for or fold into phase G.
- **Category**: language / runtime / editor
- **Planned at**: 2026-09-26

## Why this matters

Game logic needs a unit that is (1) per instance, (2) configured by props from code or
the editor, (3) edited while the game runs. Today that's editor behaviors: the fields
live in `j3.editor.toml`, the values are read by name at runtime (`scene3d.number(...)`),
and dispatch is a generated `src/behaviors.j3`. None of it is typed, and every code edit
rebuilds and relaunches the game (~0.7 s, EDITOR.md "Performance > Builds").

The design is agreed and written up in `speculative.j3s`. Scripts are first class: same
parser, same checker, same diagnostics and LSP as J3. **Editing a script never recompiles
the program**: the program binary is built once, and scripts run on a bytecode VM, so a
script edit re-runs instantly (or swaps into the running program). `j3c build` still
makes one self-contained binary with every script compiled AOT and no VM.

## The language (from `speculative.j3s`)

```
// scripts/door.j3s
use j3.math.{ clamp, lerp };

props {
    hinge: Node;             // required: no default
    openAngle: f32 = 90.0;
}

shared {
    doorsOpened: int = 0;    // one value for every instance of this script
}

let open = false;            // per-instance state
let t = 0.0;

pub fn update(scene: &Scene, dt: f32) { ... t = ...; ... openAngle ... }
pub fn toggle() { open = !open; }
fn smooth(x: f32): f32 { ... }   // private
```
```
// main.j3
const Door = @script("scripts/door.j3s");     // path from project root
let vault = Door { hinge: h, openAngle: 120.0 };
vault.toggle();
```

Rules: no `module` line. `props` are read-only in the script, and a prop with no default
is required. `shared { }` holds values every instance of the script reads and writes (the
script's globals). Top-level `let`s are per-instance state. Every default and initializer
is a literal. Props, shared, lets, and fns share one namespace. Outside code sees only
`pub` fns: no field access to a script instance, and instances can't be copied (pass
`&Door`). The only top-level items allowed are `use`, `const`, `props`, `shared`, `let`,
and `fn`.

## Current state

- Pipeline `parse → check → ir → emit LLVM | C` (`j3c/AGENTS.md`). "New printers plug
  into `emit::Backend` and consume the same typed IR." IR is fully typed and uses places;
  field offsets come from `ir/abi.rs` (SysV LP64).
- `@ident(...)` is compiler-call syntax. The only call today is `@syscall`
  (`parse/syntax.rs:16`, `INTRINSIC_SYSCALL`).
- Struct field defaults exist and must be literals (`check/mod.rs:488-503`, `field_default` at
  `check/mod.rs:2366`). `Place` has `Access` (read-only vs writable), which
  `ir/validate.rs` enforces.
- Modules are found by dotted name (`compile.rs:399` `module_file`, `:413` `join_mod`), and
  cycles are checked in `compile.rs:318` `check_import_cycles`.
- Behaviors: `editor.rs:866` `generate_dispatch` writes `src/behaviors.j3`, with a
  `same(name, "...")` chain per behavior. Fields are declared in `j3.editor.toml`
  `[behavior.<name>] fields = [...]` and stored as `param.<id>.<field>` lines. Nodes store
  `node.<i>.behavior` (`stdlib/j3/scene3d.j3:58,249,1205`).
- Editor Play (`editor.rs:1218` `play`) builds the whole game and relaunches it.
- A full check of `examples/editor_village` takes 0.15 s wall.
- `j3c run` on a project skips emit and clang when no source changed (`cli.rs:599-630`,
  `source_key`/`up_to_date`), so an unchanged binary already relaunches instantly.
- `j3c/AGENTS.md` "Do not build" lists a *comptime* interpreter. This plan adds a
  *runtime* bytecode VM, which is a different thing. Phase D updates AGENTS.md to say so.

## Architecture

**A script is a module plus a struct. Lowering does the work, not a new language.**

1. The checker lowers `door.j3s` to a module with one synthesized struct. The fields are
   the props, then the state `let`s, laid out like any struct. `shared` values become
   module globals. Every script fn becomes a method with a hidden `this: &Door` param 0.
   A bare prop name lowers to `(*this).f` with read-only `Access`, a state name to the
   same place writable, a shared name to the global. After check it's ordinary IR, so
   **LLVM and C printers are unchanged**. Release builds pay nothing.
2. **Bytecode is a third printer over that same IR** (`emit/bc.rs`). Nothing parses J3 a
   second time, and the semantics are the checker's.
3. **The VM is written in J3** (`stdlib/j3/script.j3`). It runs on every target, the
   editor links it with no Rust at runtime, and it depends only on `j3.memory`/`j3.sys`,
   so non-graphics programs can use it.
4. **Same memory layout in both worlds.** Bytecode addresses fields using `abi.rs` offsets,
   and `&Scene`, slices, and value structs are real native memory. Script → J3 calls pass
   pointers and values as-is: no boxing, no marshalling.
5. **Outside code can't see inside an instance.** It calls `pub` fns and nothing else
   (no field access, no copies). That's what lets the two modes below store instances
   differently without any J3 code noticing.
6. **Two modes:**
   - **Dev** (`j3c run`, editor Play): the program binary links the VM, and every
     script type is an opaque pointer to an instance the VM allocates. `Door { ... }`
     becomes `script.new(id, props)`, and `vault.toggle()` becomes a VM call. The binary
     never knows any script's layout or code, so **any script edit, props and state
     included, needs no program compile**: j3c re-checks, writes `.j3sb` files, and
     relaunches the same binary. A code-only edit also swaps into the running program.
     A program rebuild happens only when a `.j3` file changes, or when a script `use`s a
     module the program doesn't load (a one-line reason says so).
   - **Release** (`j3c build`): one binary with every script compiled AOT, instances
     stored inline like structs, no VM, no `.j3sb` files.
   - `check` / LSP are the same as today. Scripts are just more modules.

## Phases

### A. Parse `.j3s` and `@script`

- `parse/`: a file ending `.j3s` parses in script mode. That mode adds a `props { name: T
  [= literal]; ... }` item, a `shared { ... }` item with the same shape, and top-level
  `let` items. It rejects `module`, `use lib`, and
  top-level statements, and each error names the rule
  ("scripts can't have a `module` line; the path names it").
- `@script("<literal path>")` is a new intrinsic in `syntax.rs`, valid as an expression
  where a type is expected (`const X = @script(...)`, `Door { }`, `script<Door>`). The
  path must be a string literal.
- `compile.rs`: `@script` paths join the module graph, keyed by the path relative to the
  root (`j3.toml` dir, or the entry file's dir without one). The internal module name is
  `script.<path with / → .>`, which gives stable C/LLVM symbol names. Script→script
  imports go through the same `check_import_cycles`.
- `fmt.rs` prints `props`/`shared` blocks. `dump-syntax` picks up `props`/`shared`/`script`.
- Verify: `cargo test` passes. Split `speculative.j3s` into
  `speculative/11_script/{main.j3, scripts/door.j3s}` with a headless `Scene` stub, and
  make it parse.

### B. Check and lower

- Synthesize the struct: prop fields first, then state fields. `shared` values become
  module globals. All defaults reuse `field_default` (literal only, decided).
- Resolve names with script scope between locals and module scope. Calling a fn inside
  the script passes `this` implicitly.
- A struct literal of a script type must give every prop without a default, and may not
  name a state field.
- `pub` fns are methods callable from outside. Non-`pub` fns are private. Outside code
  can't read a script instance's fields or copy an instance by value.
- Diagnostics, rustc-shaped via `diag.rs`, each with a help line:
  - missing required prop: points at the literal, notes the `props` line, suggests `hinge: ...`
  - `open` is script state, not a prop: suggests moving it into `props`
  - assigning a prop: "props are read-only in the script; copy it into a `let`"
  - calling a private fn: "`smooth` is private to scripts/door.j3s; add `pub`"
  - bad path: "no script at scripts/dor.j3s", with did-you-mean from the `.j3s` files under the root
  - `@script` path not a literal / not ending `.j3s`
  - top-level `let` / `shared` / prop default not a literal
  - reading a field of a script instance from outside: "script fields are private; add a
    `pub fn`"
  - copying an instance: "script instances can't be copied; pass `&Door`"
- Verify: `speculative/11_script` compiles and runs under LLVM and C (a `compile.rs` test,
  like `01`–`08`, and an IR golden). Negative tests cover each diagnostic above.

### C. LSP and VS Code

- `editors/vscode`: register `.j3s` as J3 (same grammar and server). Add `props` to the
  grammar via `dump-syntax`.
- `lsp/`: completion of `.j3s` paths inside `@script("`. Inside `Door { }`, complete props
  with required ones first, showing type and default. In a script, bare prop and state
  names get "prop"/"shared"/"state" in the detail. Hover on a script type shows its props and `pub`
  fn signatures. Goto on the path string opens the file. Signature help works on script
  methods. Outline shows `props`, `shared`, and state.
- `j3c doc scripts/door.j3s` prints props and `pub` fns.
- Verify: LSP tests in `lsp/` for path completion, prop completion, hover, and goto.
  Manually confirm in Cursor on `speculative/11_script`.

**A–C are shippable on their own**: typed, per-instance scripts with full tooling, AOT
only.

### D. Bytecode printer (`emit/bc.rs`)

- Input is the checked IR for script modules. Output is `target/j3s/<path>.j3sb`.
- Format, versioned header: a layout hash (instance fields and types), `pub` fn signatures,
  a constant pool, a native import table, and fn bodies.
- A register machine: each IR local is a frame slot, 8 bytes (bigger values are addressed
  in a frame byte area). Ops are typed from the IR (`add.i64`, `add.f32`, `lt.f32`, and so
  on), so the VM never checks types at runtime.
- Places lower to address arithmetic with `abi.rs` offsets: `load.f32 r, [base + off]`,
  `index` with a bounds check that emits the same `Abort` as native. `--fast` strips it
  the same way.
- Fused ops for the hot shapes: field load/store off `this`, compare-and-branch,
  `AssignOp` on a place.
- Native calls use a stable key (module path, fn name, signature hash), not `FuncId`, so
  keys survive rebuilds.
- Update `j3c/AGENTS.md`: bytecode is a printer, and the runtime VM is allowed (the
  comptime interpreter still isn't).
- Verify: `j3c dump-bc speculative/11_script/scripts/door.j3s` prints readable bytecode.
  A golden locks it.

### E. VM (`stdlib/j3/script.j3`)

- Loads a `.j3sb` and runs a `switch` dispatch loop over a `u8` opcode.
- Frames live on one preallocated VM stack in the VM's arena, so a call does no
  allocation. The VM never allocates while running.
- Instances: `script.new(id, props)` allocates the instance and the script's `shared`
  block in the VM's arena. On a relaunch with a new layout, nothing old survives, so
  there's nothing to migrate.
- Native calls: the dev build emits a **thunk table**. Each thunk is a J3-ABI function
  `fn(args: &u8, ret: &u8)` that loads args at known offsets, calls the real fn, and
  stores the result. There's one thunk for every `pub` fn in every module the program
  loads, so a script edit can call anything the program has without a rebuild. A script
  that needs a module the program doesn't load triggers a rebuild ("`j3.gfx.noise` isn't
  in the program; rebuilding").
- Script → script calls stay inside the VM.
- Benchmarks (`j3c/bench/script/`): a loop of int and f32 math, a call-heavy recursive
  fn, a field-heavy update over 10k door instances, and a native call round trip. Compare
  with the same code AOT at `-O2`.
- Targets: the VM runs straight-line math ≤ 10× slower than native; a native call costs
  ≤ 50 ns; the VM is ≤ 2500 lines of J3; the VM adds ≤ 150 KB to a binary.
- Verify: every `speculative/11_script` test also passes through the VM (a test mode
  that forces all script calls into the VM). Benchmarks print a table.

### F. Dev mode

- `j3c run` on a program that uses scripts builds it in dev mode. Script types lower to
  an opaque pointer, `Door { ... }` to `script.new`, and `pub` fn calls to
  `script.call(id, fn, args)`. The VM and the thunk table are linked. The dev binary's
  cache key (`source_key`) covers `.j3` files and the stdlib, **not** `.j3s` files.
- Every `j3c run`: check the whole program. If only `.j3s` files changed, write their
  `.j3sb` files to `target/j3s/` and launch the cached binary. No emit, no clang. The
  program loads bytecode from `target/j3s/` under `J3_ROOT`.
- If a script's `pub` fn signature changed, the J3 code calling it no longer
  type-checks, so that's a check error until the `.j3` side is fixed, which then rebuilds.
- Live swap while running: j3c (or the editor) watches `.j3s` files. When the layout
  hash and `pub` signatures are unchanged, it writes the new `.j3sb`. The program swaps
  code at a frame boundary (`j3.app`'s loop calls `script.poll()`; a non-app program
  calls it itself), and instances keep their memory. When the layout changed, it
  relaunches the same binary. A check error prints and keeps the old code running.
- Verify, with an automated headless test:
  - Build once, then edit a script constant, add a prop with a default, and add a
    state `let`. Each edit runs the new behavior with no clang call (assert via
    `J3C_TIMINGS=1`), in under 300 ms from `j3c run` to the program's first line.
  - While running, a constant edit shows up within 1 s.
  - Adding a `use` of a module the program doesn't load prints the rebuild reason and
    rebuilds.

### G. Scene host and editor

- Scene format: `node.<i>.script s=<path>`. Prop values reuse `param.<id>.<prop>`
  lines. `object` props become `Node`.
- `j3.scene3d` gets `Node` (a handle struct over the stable id) and `scene.find(path)`.
- The build scans `scenes/*.j3scene` for script paths and compiles them. The compiler
  synthesizes a registry module (in memory, not a generated source file): one instance
  array per script type, filled from params at scene load. It also provides
  `startAll`/`updateAll` and `scene.script<T>(node): Option<&T>`.
- Hooks: `pub fn start(scene: &Scene)` and `pub fn update(scene: &Scene, dt: f32)`,
  both optional. A script attached in a scene with a wrong hook signature is a
  `j3c check` error that points at the scene file line. A param whose type doesn't match
  its prop is also an error, with the scene line.
- Editor: the inspector reads props from the script (`j3c -e script-props <path>`, JSON:
  name, type, default, required) instead of `j3.editor.toml`. "Create script" writes a
  `.j3s` with a `props` block and `start`/`update`. Play uses dev mode, so a script edit
  never recompiles the game: a code change swaps live, and a layout change relaunches the
  same binary.
- Migration: convert `examples/editor_village` and `3d-editor-scene` behaviors to
  scripts. Then delete `generate_dispatch`, `create_behavior`, `behavior_names`,
  `node.behavior`, `[behavior.*]`, and the `scene3d.number`/`flag`/`label`/... getters
  if nothing else uses them.
- Verify: the village runs with scripts. Editing a script during Play changes behavior
  without a clang call. `j3c check` flags a bad param type with the scene line.

### H. Non-graphics and docs

- `examples/scripted/`: a headless program (for example a turn-based rules engine or a
  text sim) with two scripts, several instances, and props from code. It runs with
  `j3c run` (dev) and `j3c build` (one binary). No SDL.
- `j3c guide` and `j3c doc lang` get a Scripts section. The `j3c new` scaffold
  `AGENTS.md` teaches scripts. Update the root `AGENTS.md` table. Delete
  `speculative.j3s` once `speculative/11_script` covers it.
- Verify: `examples/scripted` in the test suite for both modes, with the same output.

## Compiler scalability

- Scripts are modules, so plan 008's per-module objects and caching apply unchanged.
- Bytecode is cached per script by hash (script source plus the interface hash of each
  module it uses). An unchanged script is never re-emitted.
- The dev thunk table covers every `pub` fn the program loads. **Gate:** if it adds more
  than 1 MB or 0.2 s of clang time to the village's dev build, narrow it to modules that
  scripts `use` plus their dependencies, and accept more rebuilds.
- A script edit re-checks the whole program (0.15 s today). **Gate:** if that check on
  the village measures > 250 ms, write a follow-up plan for checking one module against
  cached interfaces. Don't build it before then.
- Registry instance arrays are per script type (contiguous), so 10k doors update in one
  tight loop, not a node walk.

## Done criteria

- `speculative/11_script` compiles and runs AOT (LLVM and C) and through the VM, with goldens.
- Every Phase B diagnostic has a test. LSP completion, hover, and goto work in `.j3s` and
  on `@script` sites.
- The VM meets the Phase E targets, or the plan reports the measured numbers and stops.
- A script edit (code, props, or state) never runs clang in dev mode, headless or in
  editor Play. Code edits swap live.
- `j3c build` makes one binary with every script AOT and no VM, and it produces the
  same output as dev mode.
- Behaviors are gone from the editor and examples.
- `cargo test` is green, and `/target` stays small (no new crates).

## STOP conditions

- The IR can't express something a script needs without a new `Stmt`/`Rvalue` kind: stop
  and report. Don't add one silently.
- The VM misses its targets by more than 3×, for example straight-line math > 30× slower
  than native.
- Layout parity fails: a struct's field offsets differ between the VM and native on any
  target in `[emit] target`.
- Phase G needs the scene file format changed beyond adding `node.<i>.script`.
- Anything would need a crates.io dependency.

## Decisions (2026-09-26)

1. **Shared state**: a `shared { }` block, like `props`, holds per-script values every
   instance reads and writes. Top-level `let` stays per instance.
2. **Initializers**: literal only, for props, shared, and state.
3. **Script edits never recompile the program** (dev mode, above). `j3c build` makes the
   single AOT binary.
4. **Runtime-loaded scripts** (mods, shipped games without j3c): out of scope.
5. **Hook names**: the scene host calls `pub fn start` / `pub fn update` by those fixed
   names when a script defines them. No attribute.
6. **VM language**: J3.

## Implementation notes (2026-09-26)

What shipped differs from the text above in these places:

- **Typed lookup is `scene.script::<Door>(node)`.** J3 writes generic calls with `::<>`;
  `scene.script<Door>(n)` parses as comparisons. The checker lowers it to the registry's
  `find__<script>` (`check/lower.rs` `scene_script_find`).
- **The handle type is `NodeId`**, because `j3.scene3d.Node` is already the object record.
  `Scene.node(handle): &Node` and `Scene.find("Door" | "Parent/Child"): NodeId` are new. A
  `node: NodeId` prop is filled with the object the script is attached to unless the scene
  sets it, so most scripts need no required prop.
- **Dev cache key**: `.j3` sources plus a dump of the lowered native IR
  (`compile::dev::native_key`), not "sources minus `.j3s`". A `pub` signature change, or a
  script calling something the binary can't reach, changes the dump and rebuilds on its own.
- **The VM gets its own `-O2` object** when the project builds below `-O2`
  (`emit::compile_llvm_split`, cached by text). At `-O0` the VM was 10× slower.
- **Thunk gate tripped and was narrowed**: every `pub` fn gave 364 thunks and +0.49 s clang on
  the village. Now: functions scripts call, plus `pub` fns of modules a script `use`s
  (102 thunks). Dev vs release clang on the village at `opt = 2`: 2.99 s vs 2.65 s, most of
  it the VM; binary +104 KB. A script that calls into a module it didn't `use` rebuilds once
  and says why.
- **Containers may move instances**: generic code (`List<Door>` growing) can copy a script
  value; user code still can't.
- **Scene registry** stores each script type's instances in an arena slice; `find` is a
  linear scan (marked `ponytail:`). Scene props support `f32`/`f64`, ints, `bool`,
  `string`, `Vec3`, and `NodeId`; negative literal defaults (`-3`) count as literals.
- **Scene checks run at load** from the script ASTs (`compile/scene_scripts.rs`): missing
  script path (did-you-mean), wrong hook signature, missing required prop, unknown prop,
  and a param whose type doesn't match, each against the scene file line.
- Not supported in scripts (bytecode errors name them): taking a function's address,
  calling a function value, `@syscall`. In dev mode a script instance crosses into native
  code only as a `&Door` param or prop; a `pub` fn can't return one.

### Measured

VM bench (`j3c/bench/script`, `j3c run`, VM at `-O2`):

| Case | Native | VM | Per op |
|------|--------|----|--------|
| int math loop | 2.1 ms | 8.5 ms (3.9×) | 4.3 ns |
| f32 math loop | 2.1 ms | 8.7 ms (4×) | 4.4 ns |
| recursive calls (fib 24) | — | — | 37 ns per call |
| native call round trip | — | — | 6.7 ns |
| 10k instances × 60 updates | 0.3 ms | 12.5 ms | 21 ns per native→script call |

VM: 1037 lines of J3, 30 KB of code. Script edit to finished `j3c run` on
`speculative/11_script`: 0.13 s wall, no clang. Village check: 74 ms (gate 250 ms).

### Tests

`compile::tests`: AOT golden + both backends, dev mode vs AOT on both backends, script
edits (constant, new prop with default, new state) never run clang, a code edit swaps into
a running program within 1 s, a new native module rebuilds, `examples/scripted` in both
modes, and the scene checks. Editor Play's live swap reuses `devrun::prepare` and
`bump_reload`; it isn't covered by an automated test (it needs the editor window).


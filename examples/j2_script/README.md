# j2_script: J2 Arena

A top-down arena game whose content is j2 scripts, read by the game at runtime.

```
j3c run examples/j2_script                     # play (opens a window; needs SDL2 + SDL2_ttf + GL)
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy ARENA_TEST=1 timeout 120 j3c run examples/j2_script --headless
j3c golden examples/j2_script                  # the screenshot golden plus the ARENA_TEST run
```

`ARENA_TEST=1` plays itself: a fixed seed (`vm.seed`), fire held, aim at the nearest enemy,
strafing, for 1200 fixed ticks. Halfway it touches `j2-scripts/hello/main.j2` (only its mtime) so
every script reloads with enemies on screen, and it adds `target/arena-test/broken/`, a script
with a typo. It prints the wave, kills, and script errors, checks them (wave 3 reached, kills,
no errors but the broken script's, the reload kept and still moves every enemy, the broken
script's error text), and exits 0 or 1. It runs per fixed tick, so it gives the same result
every run.

j2 is a scripting language for **people without the source**: players, modders, designers. `.j3s` scripts are compiled by `j3c`
(and compiled into release builds), so a shipped game can't take new ones. j2 is read from
plain text **by the game itself at runtime**. The lexer, parser, compiler, and VM live in
the stdlib (`j3c/stdlib/j3/j2/`), written in J3. It looks like J3 (`let`, `fn`, `{ }`,
backtick strings, `match`, `use`) and behaves like Lua (dynamic values, tables, closures, GC).

J2 Arena is a top-down shooter. The host (J3) does the window, player, bullets, and
collision. Everything else is scripts:

| Script | Adds | Shows |
|---|---|---|
| `j2-scripts/base/` | pistol, shotgun, grunt, waves, HUD | `pub` helpers, `this` methods, `match`, `filter`/`pick`, functions in tables |
| `j2-scripts/hello/` | prints on kills | the smallest script (copy this one) |
| `j2-scripts/slimes/` | a hopping slime that splits on death | `use base;`, enemy hooks, spawn overrides, `nil` fields |
| `j2-scripts/turrets/` | orbiting drones that level up | "classes" from functions, closures with state, `map`/`join` |

Script writers read `SCRIPTING.md`: the language on one page and the game's API.

## Files

```
j3.toml
src/main.j3      App: boot makes the VM, loads scripts, ticks and draws
src/scripts.j3   script loader: script roots, load order, `use`, change detection, on-screen log and errors
src/api.j3       every native a script can call (bind table), nothing else is reachable
src/world.j3     player, bullets, enemies (j2 tables), collision, reload and restart
src/arenatest.j3 ARENA_TEST: fake input, the mid-run reload, the broken script, the checks
j2-scripts/*/main.j2 the scripts
assets/font.ttf  DejaVu Sans, for the HUD and the log
golden.toml, golden/arena.png   `j3c golden` (frame 300 of the ARENA_TEST run)
SCRIPTING.md     for script writers
```

## The `j3.j2` API it uses

```j3
use j3.j2;
use j3.j2.{ Vm, Limits, Args, Value, Ref, Error };   // J3 can't write `j2.Vm` as a type

let vm = Vm.new(mem, Limits { heapBytes: 32 << 20, stepsPerCall: 500000 });   // maxDepth: 200 too
vm.openLibs(j2.LIB_BASE | j2.LIB_MATH | j2.LIB_STRING | j2.LIB_ARRAY); // or j2.LIB_ALL
vm.host = w as ptr;                          // natives get it back with `vm.host as &World`
vm.bind("spawn", spawn);                     // fn spawn(vm: &Vm, args: Args): Value
vm.bind("print", print);                     // a bind replaces a built-in (here: print on screen)
vm.seed(1234);                               // pick/rand/math.random sequence (replays, tests)
vm.resolver = resolveUse;                    // fn(ctx: ptr, vm: &Vm, name: string): Value, for `use name;`
vm.resolverCtx = ctx as ptr;

match vm.loadFile("slimes", "j2-scripts/slimes/main.j2") {   // read + compile + run top level
                                                          // vm.loadString(name, text) for text from anywhere
    Result.Ok(pubTable) { ... }
    Result.Err(e) { e.message; e.chunk; e.line; e.col; e.trace; }
}
vm.call(f, [j2.num(dt)]);                    // Result<Value, Error>; j2.noArgs() for none (no `[]` in J3)
vm.callWithThis(f, self, args);              // binds `this` if f's first param is `this`

let r = vm.keep(v);   vm.value(r);   vm.get(r);   vm.release(r);    // GC roots held by the host
vm.getNum(r, "x");    vm.setNum(r, "hp", 3);           // fast f32 field access on a kept table
vm.get(r).getNum("color", 0.0)               // f64: exact for ints such as colors (f32 is not)
vm.newTable();  vm.newArray(n);  t.set(k, v);  t.get(k);  t.merge(other);  t.getNum(k, d);  t.getStr(k, d);
args.num(i)  args.int(i)  args.str(i)  args.table(i)  args.func(i)  args.numOr(i, d)  args.intOr(i, d)  args.count()  args.joined(sep)
vm.raise("message")                          // a script error at the caller's line; `return vm.raise(..)`
vm.intern(s)                                 // a string that lives as long as the VM (names the host keeps)
j2.nil()  j2.none()  j2.num(x)  j2.int(n)  j2.str(vm, s)
vm.stats()                                   // liveObjects, bytes, collections (a debug HUD)
```

`e.message` reads ``j2-scripts/slimes/main.j2:14:9: expected `}`, found end of file`` and, like `e.trace`, is valid
until the VM's next error: copy it to keep it. `e.chunk` (the name given to `loadFile`) stays valid.
`args.func` is `fn` in the plan; `fn` is a J3 keyword.

A native's argument getters raise a script error on a wrong type, so natives don't check
types themselves. Errors carry `chunk:line:col`, so the host can tell which script broke.

## Host notes

- Natives run inside a script call: `vm.newTable`/`newArray`/`j2.str` results stay alive until
  the native returns; `keep` anything the host holds longer (enemy tables, weapons, exports).
- A reload releases every script's exports, the kinds table and the weapons, then loads again.
  Enemy tables are the World's `keep`s, so enemies survive it and find their kind's new code by
  name (`world.j3`, `reloadScripts`).
- Errors: `scripts.j3` `disable` copies `e.message` into the script's own `Text` before anything
  else touches the VM. The on-screen log is a ring of 8 reused `Text`s, so printing allocates
  nothing new once the lines are as long as before.
- Backtick strings in J3 live in their function's frame and hold at most 512 bytes; build log
  lines and paths with `j3.text.Text` instead (error text can be longer).

Not yet: coroutines (`wait(1.5)` style scripts), metatables or operator overloading,
`try`/`catch` inside j2, sharing one `Vm` across threads, a bytecode cache, state kept across a
reload (a reload restarts every script's top-level variables, waves included).

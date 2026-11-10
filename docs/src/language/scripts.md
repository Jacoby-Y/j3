# Scripts (.j3s)

A `.j3s` file is a **script**: J3 checked by the same type checker, used for per-object game logic. During development `j3c run` executes scripts on a bytecode VM, so editing one takes effect without recompiling the game. `j3c build` compiles every script into the binary like any other code.

## Anatomy of a script

A script has no `module` line; its path names it. Each instance of a script has its own props and state.

```rust
// scripts/door.j3s
use j3.math.{ clamp, lerp };
use j3.io.printLn;
use j3.scene3d.{ Scene, NodeId };

props {
    node: NodeId;          // the object the script is on (see below)
    openAngle: f32 = 1.57;  // radians
    seconds: f32 = 0.5;
}

shared {
    doorsOpened: int = 0;
}

let open = false;
let t = 0.0;

pub fn start(scene: &Scene) {
}

pub fn update(scene: &Scene, dt: f32) {
    let goal: f32 = if open { 1.0 } else { 0.0 };
    let step = dt / seconds;
    t = clamp(t + clamp(goal - t, 0.0 - step, step), 0.0, 1.0);
    scene.node(node).rotation.y = lerp(0.0, openAngle, smooth(t));
}

pub fn toggle() {
    open = !open;
    if open { doorsOpened += 1; }
}

pub fn report() {
    printLn(`door: open {open} t {t} opened {doorsOpened}`);
}

// Private: only this script can call it.
fn smooth(x: f32): f32 {
    return x * x * (3.0 - 2.0 * x);
}
```

- **`props`** are what the creator passes in. They are read-only inside the script. A prop with no default is required.
- **`shared`** is one value every instance reads and writes.
- **Top-level `let`** is state that belongs to each instance.
- Only **`pub fn`** can be called from outside.

All defaults and initializers must be literals. A script may declare its own `struct`s and `enum`s; they share the script's namespace, and the script's own type name is taken.

## Using a script from J3

`@script` gives you the script's type:

```rust
const Door = @script("scripts/door.j3s");   // path from the project root

let door = Door { node: n };    // props by name; ones with defaults can be left out
door.toggle();
```

Outside code cannot read an instance's fields or copy one; pass `&Door` around. A script can hold other scripts the same way. `j3c doc scripts/door.j3s` summarizes a script's props and functions. `j3c doc recipes script` is a complete small project.

## Scripts on scene objects

In an editor scene, `script=scripts/door.j3s` on an object's line attaches a script, and `param.<prop>=value` sets its props. A `node: NodeId` prop receives the object it is attached to.

| Prop type | Scene line |
|---|---|
| `f32`, `f64` | `param.seconds=1.5` |
| `int` and sized ints | `param.count=3` |
| `bool` | `param.locked=1` |
| `string` | `param.label="text"` |
| `Vec3` | `param.offset.x=0.5` (and `.y`, `.z`) |
| `NodeId` | `param.target=#12` (an object id) |

A required prop (no default) must be set on every object the script is on; `j3c check` points at the scene line when one isn't, or when a value has the wrong type. A `NodeId` prop follows copies: copy an object together with the one it points at and the copy points at the other copy.

The game runs attached scripts through `j3.scene3d.scripts`, which `j3c` generates from the project's scenes:

```rust
use j3.scene3d.scripts;

let scene = scene3d.load(scripts.scene(), mem);  // the scene Play picked, else j3.editor.toml's
scripts.start(&scene);          // makes every instance, calls `pub fn start(scene: &Scene)`
scripts.update(&scene, dt);     // every frame: `pub fn update(scene: &Scene, dt: f32)`
let d = scene.script::<Door>(node);
```

`j3c new mygame --template fps` and `j3c doc recipes scene3d` already wire this up.

## Hot reload

`j3c run` (and the editor's Play) builds in dev mode and runs scripts on the VM. Editing a script's code while the game runs swaps the new code in at the next frame. Changing a script's props or state relaunches the same game binary. Neither recompiles the game. `j3c build` compiles every script in, so a shipped game cannot take new `.j3s` files; for that use [j2](j2.md).

## What scripts leave out

Scripts are checked by the same type checker but run on a smaller VM, so some features are J3-only: generic functions, traits and `#[impl]` lookup, closures, SIMD types, and the shift operators (`& | ^ ~` work). Using one is a compile error.

## Scripts as tools

A script with a `pub fn main()` can be run once from the command line, which makes it handy for generators and project checks:

```text
j3c run tools/arena.j3s some args
```

It makes one instance and calls `pub fn main()`. Arguments are read with `j3.sys.arg` (`arg(0)` is the program). A scene script is still attached with `script=`, not run this way. Typical uses are a scene generator (`tools/arena.j3s`) and a project check (`tools/check.j3s`). `j3c dump-bc file.j3s` shows the bytecode.

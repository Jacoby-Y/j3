# Scripts (.j3s)

A `.j3s` file is a **script**: J3 checked by the same type checker, used for per-object game logic. During development `j3c run` executes scripts on a bytecode VM, so editing one takes effect without recompiling the game. `j3c build` compiles every script into the binary like any other code.

## Anatomy of a script

A script has no `module` line; its path names it. Each instance of a script has its own props and state.

```rust
// scripts/door.j3s
use j3.math.{ clamp, lerp };
use j3.io.printLn;
use scene.{ Scene, Node };

props {
    hinge: Node;
    openAngle: f32 = 90.0;
    seconds: f32 = 0.5;
}

shared {
    doorsOpened: int = 0;
}

let open = false;
let t = 0.0;

pub fn update(scene: &Scene, dt: f32) {
    let goal: f32 = if open { 1.0 } else { 0.0 };
    let step = dt / seconds;
    t = clamp(t + clamp(goal - t, 0.0 - step, step), 0.0, 1.0);
    scene.setAngle(hinge, lerp(0.0, openAngle, smooth(t)));
}

pub fn toggle() {
    open = !open;
    if open { doorsOpened += 1; }
}

pub fn report() {
    printLn(`door {hinge.id}: open {open} t {t} opened {doorsOpened}`);
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

All defaults and initializers must be literals. A script may declare its own `struct`s and `enum`s.

## Using a script from J3

`@script` gives you the script's type:

```rust
const Door = @script("scripts/door.j3s");   // path from the project root

let door = Door { hinge: h };   // props by name
door.toggle();
```

Outside code cannot read an instance's fields or copy one; pass `&Door` around.

## Scripts on scene objects

In an editor scene, `script=scripts/door.j3s` on an object's line attaches a script, and `param.<prop>=value` sets its props. A `node: NodeId` prop receives the object it is attached to.

```rust
use j3.scene3d.scripts;

scripts.start(&scene);          // calls each script's `pub fn start(scene: &Scene)`
scripts.update(&scene, dt);     // calls `pub fn update(scene: &Scene, dt: f32)`
let d = scene.script::<Door>(node);
```

## Scripts as tools

A script with a `pub fn main()` can be run once from the command line, which makes it handy for generators and project checks:

```text
j3c run tools/arena.j3s some args
```

Arguments are read with `j3.sys.arg` (`arg(0)` is the program). `j3c dump-bc file.j3s` shows the bytecode.

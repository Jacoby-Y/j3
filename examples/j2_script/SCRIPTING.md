# Scripting J2 Arena

Scripts are plain text files. You don't need any tools or the game's source. Use any text
editor.

## Your first script

1. Open the `j2-scripts` folder next to the game.
2. Copy the `hello` folder and rename the copy, for example to `myscript`. Use letters,
   digits and `_` only, since other scripts refer to yours by this name.
3. Start the game and edit `j2-scripts/myscript/main.j2`. Every time you save, the game reloads
   all scripts. You don't need to restart.

What `print(...)` writes and every error show up at the bottom-left of the screen. If your
script has a mistake, the game says which one and where, for example:

```
script myscript disabled: j2-scripts/myscript/main.j2:12:5: expected `}`, found end of file
```

The rest of the game keeps running. Fix the line and save.

**Controls:** WASD moves. Hold the left mouse button to shoot. 1-9 switch weapons. F5
reloads scripts. R restarts after game over.

**Units:** positions are pixels from the top-left of a 1100 x 700 arena, with y going
down. Angles are radians, time is seconds, and speeds are pixels per second.

## The one rule

The game calls the functions whose names start with `on`. That covers your script's own
`pub fn onTick(dt)` and the `onTick` inside an enemy you define. Everything else is yours
to name.

## j2 in one page

j2 looks like J3 and works like Lua. You don't write types, because values carry their
own.

```
// a comment
let hp = 10;           // a variable
const MAX = 3;         // a variable that can't change
hp += 1;

let name = "slime";
print(`{name} has {hp} hp`);       // backtick strings fill in {expressions}
print("plain " + name);            // + joins strings

let p = { x: 1, y: 2 };            // a table: names -> values
p.z = 3;                           // add a field
p.z = nil;                         // remove it
print(p.w);                        // a missing field is nil, not an error

let xs = [10, 20, 30];             // an array; the first element is xs[0]
xs.push(40);
print(len(xs));                    // 4

if hp > 5 && name != "grunt" { } else if !done { } else { }
while hp > 0 { hp -= 1; }
for i in 0..3 { }                  // i = 0, 1, 2
for v in xs { }                    // each value
for i, v in xs { }                 // index and value
for k, v in p { }                  // each field name and value, in the order they were added
// break and continue work in loops

match n {                          // compare against ints, strings, true/false
    1 { }
    "two" { }
    _ { }                          // anything else
}

fn add(a, b) { return a + b; }     // a function; without `return` it gives nil
let half = fn(v) { return v / 2; };          // a function stored in a variable
let counter = fn() {
    let n = 0;
    return fn() { n += 1; return n; };       // remembers n between calls
};
```

Good to know:

- **True and false:** only `nil` and `false` count as false. `0` and `""` count as true.
- **Numbers:** there are ints (`3`) and floats (`3.0`, `1e9`). An int divided by an int
  gives an int (`7 / 2` is `3`). If either side is a float, the result is a float
  (`7.0 / 2` is `3.5`). `float(x)` and `int(x)` convert.
- **`this`:** writing `obj.f(a)` hands `obj` to `f` as `this`, but only when `f`'s first
  parameter is named `this`. Otherwise it's a plain call. That's how you write methods
  (see `j2-scripts/turrets`).
- **Dot calls:** when `x` has no field `f`, `x.f(a)` means `f(x, a)`. So `xs.push(v)` is
  `push(xs, v)` and `name.upper()` is `upper(name)`.
- **Private and shared:** the top-level names in your file belong to your script. Put `pub`
  in front of a `fn`, `let` or `const` to share it.
- **Other scripts:** `use base;` gives you base's `pub` names, for example `base.chase(e, dt)`.
  The game loads base first if it hasn't yet.
- **Typos:** a name that doesn't exist (`spwan(...)`) is an error as soon as the script
  loads, with its line. A missing *field* is just `nil`.
- **Errors:** `error("message")` stops your script and shows the message. A loop that never
  ends does the same after a moment, so it can't freeze the game.

## Built-ins

| Name | Does |
|---|---|
| `print(a, b, ...)` | Writes the values on one line, bottom-left of the screen |
| `len(x)` | The size of an array, string or table |
| `type(x)` | `"nil"` `"bool"` `"int"` `"float"` `"string"` `"array"` `"table"` `"fn"` |
| `str(x)` `int(x)` `float(x)` | Convert |
| `error(msg)` | Stop with an error |
| `push(a, v)` `pop(a)` `insert(a, i, v)` `remove(a, i)` | Change an array |
| `pick(a)` | A random element, or nil when empty |
| `rand(lo, hi)` | A random float from `lo` up to (not including) `hi` (also `math.random(lo, hi)`) |
| `map(a, f)` `filter(a, f)` `find(a, f)` `sort(a)` `sort(a, less)` `join(a, sep)` | A new array, the first match, sort in place (`less(x, y)` decides the order; default `<`), text with `sep` between values |
| `keys(t)` `has(t, name)` | A table's field names; whether it has a field |
| `upper(s)` `lower(s)` `trim(s)` `split(s, sep)` `contains(s, part)` `slice(s, from, to)` | Text |
| `math.pi` `math.sqrt` `sin` `cos` `tan` `atan2` `abs` `floor` `ceil` `min` `max` `clamp` `lerp` `random` | Math, e.g. `math.clamp(v, 0, 1)`. `floor` and `ceil` give ints; `abs` `min` `max` `clamp` keep ints ints |

## The game

### Script hooks

Write any of these as `pub fn` in your `main.j2`.

| Hook | Called |
|---|---|
| `onStart()` | Once all scripts have loaded. It runs again after every reload |
| `onTick(dt)` | 60 times a second, with `dt` in seconds |
| `onDraw()` | Every frame, after the game draws, so you can draw a HUD on top |
| `onKill(enemy)` | After an enemy dies, with that enemy's table |

### Functions

| Function | Does |
|---|---|
| `defineEnemy(name, kind)` | Adds an enemy kind (see below). Using a name again replaces that kind |
| `defineWeapon(name, weapon)` | Adds a weapon on the next free number key (see below). Using a name again replaces it |
| `spawn(kind, x, y)` | Makes an enemy and returns its table. Returns nil if the arena is full (512) |
| `spawn(kind, x, y, fields)` | The same, with `fields` replacing the kind's values for this one enemy |
| `shoot(x, y, vx, vy)` / `shoot(x, y, vx, vy, dmg)` | Fires a player bullet. Damage defaults to 1 and bullets live 2 seconds |
| `enemies()` | All live enemies, as an array of their tables |
| `player()` | `{ x, y, hp, weapon }`, where `weapon` is the current weapon's name |
| `score()` | The score |
| `arena()` | `{ w, h }` |
| `rgb(r, g, b)` | A color (an int, `0xRRGGBB`). Each part is 0-255 |
| `drawText(x, y, text)` / `drawText(x, y, text, color)` | Draws text. Use the draw functions only inside `onDraw` |
| `drawCircle(x, y, r, color)` `drawRect(x, y, w, h, color)` | Draw shapes |

The game has no files, network or other programs available to scripts. These functions are
all there is.

### Enemies

```
defineEnemy("bat", {
    hp: 4,             // health; the enemy dies at 0          (default 10)
    r: 10,             // radius in pixels, used for hits      (default 14)
    speed: 120,        // only your own code reads this        (default 60)
    touch: 20,         // damage per second while touching you (default 10)
    score: 3,          // points for the kill                  (default 1)
    color: rgb(150, 100, 255),

    onSpawn: fn(this) { },           // right after spawn(); `this` is the new enemy
    onTick: fn(this, dt) { },        // 60 times a second; move it by changing this.x / this.y
    onHit: fn(this, dmg) { },        // a bullet hit it (hp is already lowered)
    onDeath: fn(this) { },           // hp reached 0
});
```

Every hook is optional. An enemy with no `onTick` just stands still. Each enemy is its
own table: it starts with a copy of the kind's numbers plus `kind` (the name), `x` and `y`.
You can add any fields you like, such as `this.timer`. The game reads `x y r hp touch score
color`, and changing those changes the enemy right away.

### Weapons

```
defineWeapon("laser", {
    cooldown: 0.05,                   // seconds between shots (default 0.2)
    onFire: fn(x, y, aimX, aimY) {    // x, y: the player; aimX, aimY: the mouse
        shoot(x, y, (aimX - x) * 4, (aimY - y) * 4, 1);
    },
});
```

`onFire` can call `shoot` as many times as it likes. Name its first parameter `this` to
read the weapon's own fields, as `base`'s shotgun does with `this.pellets`.

### Building on `base`

`base` is the game's own content, written as a script like yours. Use `use base;` to get:

| Name | Does |
|---|---|
| `base.chase(enemy, dt)` | Moves an enemy toward the player at `enemy.speed` |
| `base.aim(x, y, toX, toY)` | `{ x, y }`, a length-1 direction |
| `base.addSpawner({ name, cost, minWave, make })` | Puts an enemy in the waves. `make(x, y)` spawns it, `cost` is how much of the wave it uses, and it shows up from wave `minWave` on |
| `base.currentWave()` | The wave number |

## Reloading

Saving any `main.j2` (or pressing F5) runs every script again from the top, in folder-name
order. Your top-level variables start over and `onStart` runs again. Enemies already on
screen stay and use the new code straight away. Enemies of a kind you removed stand still.

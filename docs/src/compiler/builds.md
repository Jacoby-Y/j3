# Builds, Backends, and Optimization

## How a build works

`j3c` reads your project, type-checks it into an intermediate representation, and prints **LLVM IR** as text. The host `clang` compiles and links that into a native binary. There is no libLLVM inside `j3c`, so it stays a small, dependency-free Rust program.

```text
parse → check → emit LLVM IR → clang → target/<name>
```

The standard library is written in J3 and embedded in the `j3c` binary, so it is compiled along with your code.

## Backends

| Backend | How to pick it | Notes |
|---|---|---|
| LLVM | default | Full feature set |
| C | `--emit c` or `[emit] backend = "c"` | Portable C11; a few features (such as nested array globals) need LLVM |

`j3c dump-llvm` and `j3c dump-c` print what each backend produces.

## Build modes

| Mode | Command | What you get |
|---|---|---|
| Dev | `j3c run`, `j3c build` | Opt 0, debug info, fast incremental rebuilds. On Linux, a crash prints a stack trace. |
| Release | `--release` | Opt `s`, no debug info, stripped. Bounds checks stay on. |
| Fast | `--fast` | Bounds-check aborts removed |

Dev builds are for the edit loop; ship release builds. As an example, a large game goes from 4.5 MB in dev to 1.3 MB in release. `j3c size` shows where the bytes go.

On Linux with `ld.lld` or `mold` available, release builds use ThinLTO: one object per module, optimized across modules at link time, with a cache.

## Dev-only code

`j3.build` tells your program how it was built. `DEV` and `RELEASE` are compile-time constants, so the dead branch is removed.

```rust
use j3.build;

if build.DEV {
    drawDebugOverlay();
}
```

## Incremental builds

At opt 0 and 1, `j3c` keeps one object file per module and only recompiles modules that changed. Set `[emit] incremental = "all"` to do this at every opt level (faster rebuilds, but no inlining across modules).

## Cross-compiling

`[emit] target` takes one triple or a list. With several, each binary goes to `target/<triple>/<name>`.

```toml
[emit]
target = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-gnu"]
```

Android has its own flow: see `j3c doc android`.

# Contributing to J3

Thanks for helping. J3 is a young, alpha project: bug reports with a small repro are as welcome as code.

## Build and test

You need Rust (stable), `clang`, and for anything with a window SDL2 and OpenGL. `j3c doctor` lists what is missing.

```sh
cd j3c
J3C_NO_INSTALL=1 cargo build --release      # binary: j3c/target/release/j3c
J3C_NO_INSTALL=1 cargo test --release --no-fail-fast
```

From the repository root, with that binary built:

```sh
tools/ci/samples.sh          # speculative/ samples and `j3c check` of every example
tools/ci/stdlib-tests.sh     # j3c/stdlib/tests/*.j3, headless (about 10 minutes)
```

CI (`.github/workflows/ci.yml`) runs all three. Never open a window in a test: use
`j3c run --headless --frames N --screenshot out.png`, and `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`
for a built binary. `j3c doc testing` has the details.

`./build.sh` builds a release `j3c` and installs it to `$J3_BIN_DIR`, else `~/.bin`, else `~/.local/bin`.

Optional: a `.cargo/config.toml` in the directory *above* your clone can set a `rustc-wrapper`
that copies each fresh `j3c` onto your `PATH` (see `j3c/.cargo/copy-j3c.sh`). `J3C_NO_INSTALL=1` turns it off
for a build. Nothing in the repository installs anything by default.

## Where code lives

- `j3c/` is the compiler (Rust). Its map and rules are in [`j3c/AGENTS.md`](j3c/AGENTS.md).
- `j3c/stdlib/j3/` is the standard library, written in J3 and embedded in the binary (rebuild `j3c` after editing it).
- `j3c/editor/` is the 3D scene editor, also in J3.
- `j3c/src/doc/pages/` and `j3c/src/helptext/` are the `j3c doc` / `j3c guide` pages; `docs/` is the book.
- [`AGENTS.md`](AGENTS.md) is the full repository map and where to find each topic.

## Rules

- No crates.io dependencies. No libLLVM. No second parser: the LSP, formatter and checker share one frontend.
- Keep `j3c doc` pages and `AGENTS.md` accurate when you change what they describe.
- Docs style: short, plain, factual sentences.

## Pull requests

- One topic per PR. Say what changed and how you tested it (commands and their result lines).
- Add or update a test for behavior changes: a Rust test in `j3c/src/**/tests.rs`, a stdlib test in
  `j3c/stdlib/tests/`, or a golden (`j3c golden`).
- `cargo test --release` passes and `tools/ci/samples.sh` is clean.
- New binary assets need a known license; add them to [`THIRD_PARTY.md`](THIRD_PARTY.md).
- By contributing you agree your work is licensed under MIT OR Apache-2.0, like the rest of the project.

Be kind; see the [Code of Conduct](CODE_OF_CONDUCT.md).

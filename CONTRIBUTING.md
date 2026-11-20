# Contributing to J3

Thanks for helping. J3 is a young, alpha project: bug reports with a small repro are as welcome as code.

## Build and test

You need `clang`, a seed `j3c` (an installed one, e.g. from a release; else Rust (stable) once to build the old
Rust compiler as the seed), and for anything with a window SDL2 and OpenGL.
`j3c doctor` lists what is missing.

```sh
J3_STDLIB=$PWD/j3c/stdlib j3c build --opt 2 j3c-bootstrap   # binary: j3c-bootstrap/target/j3c
j3c-bootstrap/tests/unit.sh                                # compiler unit tests
```

From the repository root, with that binary built:

```sh
tools/ci/samples.sh          # speculative/ samples and `j3c check` of every example
tools/ci/stdlib-tests.sh     # j3c/stdlib/tests/*.j3, headless (about 10 minutes)
```

CI (`.github/workflows/ci.yml`) runs them. Compiler changes also run `j3c-bootstrap/tests/diff.sh` (`j3c-bootstrap/tests/README.md`). Never open a window in a test: use
`j3c run --headless --frames N --screenshot out.png`, and `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`
for a built binary. `j3c doc testing` has the details.

No `j3c` yet: `J3_SEED=$(j3c-bootstrap/tools/stage0.sh rust-final) ./build.sh` (needs cargo; the
seed is cached in `~/.cache/j3/stage0/`). `./build.sh` builds `j3c` (the seed builds stage 1, stage 1 builds stage 2) and installs it to `$J3_BIN_DIR`, else
`~/.bin`, else `~/.local/bin`. Nothing else in the repository installs anything.

## Where code lives

- `j3c-bootstrap/` is the compiler, written in J3. Its map and rules are in [`j3c-bootstrap/AGENTS.md`](j3c-bootstrap/AGENTS.md).
- `j3c/stdlib/j3/` is the standard library, written in J3 and embedded in the binary (rebuild `j3c` after editing it).
- `j3c/editor/` is the 3D scene editor, also in J3.
- `j3c-bootstrap/text/` holds the `j3c doc` / `j3c guide` pages; `docs/` is the book.
- [`AGENTS.md`](AGENTS.md) is the full repository map and where to find each topic.

## Rules

- No crates.io dependencies. No libLLVM. No second parser: the LSP, formatter and checker share one frontend.
- Keep `j3c doc` pages and `AGENTS.md` accurate when you change what they describe.
- Docs style: short, plain, factual sentences.

## Pull requests

- One topic per PR. Say what changed and how you tested it (commands and their result lines).
- Add or update a test for behavior changes: a compiler case in `j3c-bootstrap/tests/cases/` or unit test in
  `j3c-bootstrap/tests/unit/`, a stdlib test in
  `j3c/stdlib/tests/`, or a golden (`j3c golden`).
- `j3c-bootstrap/tests/unit.sh` passes and `tools/ci/samples.sh` is clean.
- New binary assets need a known license; add them to [`THIRD_PARTY.md`](THIRD_PARTY.md).
- By contributing you agree your work is licensed under MIT OR Apache-2.0, like the rest of the project.

Be kind; see the [Code of Conduct](CODE_OF_CONDUCT.md).

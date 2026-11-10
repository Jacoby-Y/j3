# Public release checklist

Goal: anyone on any supported system can clone, build or install, and run J3 in five minutes, before
the first Steam game ships. Scored audit with more detail: [version1](version1.md).

## 1. Clone-and-build blockers (a fresh clone fails today)

- [x] **`j3c/.cargo/config.toml`**: hardcodes `rustc-wrapper = /home/jacoby/...` and `-fuse-ld=mold`. A fresh `cargo build` fails on any other machine. Drop both (or move to an untracked `~/.cargo/config.toml`).
- [x] **`.cargo/copy-j3c.sh` auto-install**: `cargo build` silently overwriting `~/.bin/j3c` is a surprise for outsiders. Keep it local-only or opt-in.
- [x] **`build.sh`**: hardcoded `/home/jacoby/.bin`. Use `${PREFIX:-$HOME/.local}/bin`, check `clang` exists, print what's missing.
- [x] **`examples/` is gitignored**: the whole folder (incl. ones AGENTS.md and the README point at) isn't in the repo. Decide which ship, un-ignore them, ignore only their `target/`.
- [ ] **Test on a clean machine**: fresh Ubuntu/Fedora/Arch container, `git clone` → build → `j3c new hello --app` → `run --headless`. Write down every missing package.
- [x] **Grep for personal paths/data**: `/home/jacoby`, emails, usernames, local machine notes in tracked files and the stdlib/docs baked into the binary.

## 2. Legal

- [x] **LICENSE**: pick one (MIT/Apache-2.0 dual is the Rust norm) and add it at the root, plus `license` in `Cargo.toml` and `editors/vscode/package.json`.
- [x] **Asset provenance**: 145+ binary assets (png/wav/ttf/glb). Each needs a known source and license, or replace it. Add `ASSETS.md`/`NOTICE` listing third-party files (Bootstrap Icons, Apache fonts, CC0 models, SDL).
- [x] **Vendored libs**: SDL2 and friends anywhere in the tree need their license files next to them.
- [ ] **Steamworks**: confirm no Valve SDK file or header copy is tracked; `steam-check` docs say "bring your own SDK".
- [ ] **Name check**: make sure "J3" doesn't collide with an existing trademark/package name on crates.io, npm, apt, Homebrew.

## 3. Repo hygiene (loose/pointless files)

- [x] **Root scratch docs**: `memory-leak.md` (personal desktop incident), `EPIC_COMPILER.md` (half done notes). Delete, or fold what's still true into `roadmap.md`.
- [x] **Agent dot-folders**: `.agents/`, `.codex/`, `.cursor/` (empty), `.claude/settings.local.json`. Remove or gitignore; keep `AGENTS.md`/`CLAUDE.md` if agent workflow is a feature.
- [x] **Stray build output**: root `target/` (`a.out`, `compilation.log`), `tools/__pycache__`, `editors/vscode/*.vsix`. Delete locally, confirm ignored.
- [x] **`j3c/examples/`**: these are really checks/tests (`*_check.j3`, `bench`). Move to `j3c/tests/` or name the folder for what it is.
- [x] **`speculative/`**: name reads as "experimental". Rename (`samples/`?) or explain it in the README; it's the main learn-by-example set.
- [ ] **Large binaries**: `lofi-loop.wav` (5 MB), voxel-editor model PNGs, 1–4 MB golden PNGs. Shrink, move to Git LFS, or drop. Consider squashing history before going public (50 MB `.git`, and old commits may hold stuff you don't want public).
- [ ] **Uncommitted work**: land or stash the current diff so the public tree is one clean state.
- [ ] **Stale references**: code comments citing deleted plans ("plan 023"), docs naming examples that don't exist. Sweep with grep. Missing-example references swept (chunk 6); `plan 0NN` comments not swept.
- [x] **`.gitignore`**: one pass so a clone + build + run leaves `git status` clean.

## 4. README and first impression

- [x] **Rewrite README for humans**: what J3 is in one paragraph, a screenshot/GIF, why it exists, status ("alpha, Linux first").
- [x] **Fix example table**: 11 of 12 listed examples don't exist. List only real ones, each with a screenshot. Screenshots only for editor_village and desk-app so far.
- [x] **Install section**: one command per platform (see §6), then build-from-source.
- [x] **5-minute quick start**: `j3c new`, edit, `run`, `build --release`.
- [x] **Hello-world snippet**: 10 lines of J3 right in the README so people see the language.
- [x] **Feature list**: AOT via clang, no deps, 3D editor, `.j3ui`, shaders in J3, Steam, Android, MCP for agents.
- [ ] **Requirements per platform**: exact package names (`apt install clang libsdl2-dev ...`).
- [x] **Links**: book, `j3c doc`, examples, roadmap, contributing, license.
- [x] **Badges**: CI, latest release, license.

## 5. Loose ends in the compiler/stdlib

- [x] **Green tests**: `cargo test` fully passes (last audit: IR golden drift, `display` recipe missing font).
- [x] **Version**: bump from `0.0.1` to `0.1.0`; one source of truth for compiler, stdlib, docs id, VS Code extension.
- [x] **Multiple errors per check**: report more than the first type error.
- [x] **Literal on the left** (`2 * far`): fix, it's the kind of thing a first user hits in minute one.
- [x] **Panics on bad input**: 500+ `.unwrap()` in `j3c/src`. At least catch panics at the top and print "internal compiler error, please report" with a link.
- [ ] **Crash fuzz pass**: throw random/truncated files at the parser and checker for a while.
- [ ] **Memory leak**: close out or turn `memory-leak.md` into an issue.
- [x] **Error messages for missing system deps**: no `clang`, no SDL2, no libGL should each give a one-line fix, not a linker dump.
- [x] **`j3c doctor`** (optional): checks clang, SDL2 libs, GL, prints what's missing per platform.
- [ ] **Hide/flag unfinished features**: anything half done (Windows/macOS paths, stubs) says so when used.

## 6. Installation / distribution

- [x] **Prebuilt binaries**: GitHub Releases with `j3c-<ver>-x86_64-linux.tar.gz` (+ aarch64, later Windows/macOS) and checksums. Workflow written (x86_64 + aarch64), not yet run on GitHub.
- [x] **Install script**: `curl -fsSL .../install.sh | sh` → drops `j3c` in `~/.local/bin`, checks for clang.
- [ ] **cargo**: `cargo install --git ...` works (needs §1 fixed; `publish = false` blocks crates.io until you want it).
- [x] **apt / `.deb`**: J3 already makes `.deb`s for games; build one for `j3c` itself. Start with a downloadable `.deb`, later a PPA or own apt repo.
- [x] **AUR**: `j3c-bin` PKGBUILD, cheapest Linux distro channel to maintain.
- [ ] **Homebrew** (once macOS runs): tap with a formula.
- [ ] **Windows** (once it runs): zip + `winget`/Scoop manifest.
- [ ] **npm** (optional): `npx j3c` wrapper that downloads the release binary (esbuild-style). Low value unless targeting web devs; skip at first.
- [ ] **Nix/Flatpak** (later): only if people ask.
- [x] **Runtime deps declared**: each package depends on clang (+ SDL2/GL dev libs as recommends).
- [x] **Uninstall + upgrade**: documented; `j3c version` tells you if a newer one exists (optional).

## 7. Platforms

- [x] **State the support matrix**: Linux x86-64 supported; others "experimental" until tested.
- [ ] **Windows**: run the built exe for real, fix what breaks, test in CI.
- [ ] **macOS**: build + run once, decide if it's in scope for v0.1.
- [ ] **Wayland vs X11, NVIDIA vs Mesa**: smoke-test both; note the ones you can't test.
- [ ] **Steam Deck / sniper runtime**: covered by [01-steam](01-steam.md), but needed before your game ships anyway.

## 8. CI and release process

- [x] **GitHub Actions**: `cargo test`, run `speculative/01–08`, `j3c check` every example, headless goldens. Headless goldens are not in CI yet; nothing verified on a real runner until the repo exists.
- [x] **Release workflow**: on tag `v*`, build binaries, `.deb`, checksums, upload to Release.
- [x] **CHANGELOG.md**: start at 0.1.0, keep it per release.
- [x] **Versioning policy**: semver-ish, "0.x may break anything; breaks noted in CHANGELOG".
- [ ] **Format version stamps**: `.j3scene`, `.j3ui`, `j3.toml` carry a version so future migrations are possible.

## 9. Docs for humans

- [x] **Publish the book**: mdBook to GitHub Pages; link from README. `.github/workflows/docs.yml`; needs Pages enabled once the repo exists.
- [ ] **Language tour + reference**: `j3c doc lang` is short; the book needs a full syntax/semantics chapter.
- [ ] **Tutorial**: build a small 2D game end to end, then a 3D scene with the editor.
- [x] **Agent-first stays, but second**: `AGENTS.md`/MCP setup gets its own page, not the front door.
- [ ] **Editor guide with screenshots**: `j3c/editor/README.md` turned into a book chapter.
- [ ] **FAQ**: why not LLVM lib, why clang, why no deps, how it compares (Odin, Zig, Jai, C3).

## 10. Community / project files

- [x] **CONTRIBUTING.md**: how to build, test, where code lives (trim `AGENTS.md`), PR rules.
- [x] **Issue/PR templates**: bug report asks for `j3c version --verbose`, OS, GPU.
- [x] **CODE_OF_CONDUCT.md**: standard Contributor Covenant.
- [x] **SECURITY.md**: where to report (the HTTP/TLS stack and j2 mod scripts make this relevant).
- [ ] **Public remote**: create the GitHub repo/org, set description, topics, social preview (logo).
- [ ] **Where to talk**: Discussions or a Discord, linked from README.

## 11. VS Code extension

- [ ] **Publish**: Marketplace + Open VSX (Cursor/VSCodium), `publisher` set, icon, README with GIF.
- [x] **Finds `j3c`**: clear message if `j3c` isn't on PATH, with an install link.

## 12. Launch

- [ ] **Showcase**: the Steam game is the best proof; a trailer/GIF of it in the README once it's out.
- [ ] **Announcement post**: what, why, what it can do, what it can't yet.
- [ ] **Dry run**: have someone who's never seen it follow the README on their machine, fix every snag.

## Chunks

Scope for this run: the "big stuff". Not done here (need the developer or are outward-facing): creating the
public GitHub repo, publishing the VS Code extension, squashing git history, real Windows/macOS runs, fuzzing,
Homebrew/winget/npm. Defaults picked: MIT OR Apache-2.0 dual license; examples ship minus build output.

1. [x] **Clone-and-build + hygiene**: portable `j3c/.cargo/config.toml` (no absolute wrapper, no mold), opt-in install script, `build.sh` with `PREFIX`, un-ignore `examples/` (ignore their build output), remove root scratch docs (fold live facts into `roadmap.md`), empty dot folders, stray `target/`/pycache, stale references, personal paths. Done when: `git clone` to a scratch dir + `cargo build --release` there succeeds with no mold and nothing written to `~/.bin`; `git status` clean after build.
2. [x] **Legal**: root `LICENSE-MIT` + `LICENSE-APACHE`, license fields in `Cargo.toml` / `editors/vscode/package.json`, `THIRD_PARTY.md` inventory of every tracked binary asset and vendored file with source+license (unknowns listed as such). Done when: every tracked png/wav/ttf/glb/etc. appears in the inventory.
3. [x] **Compiler loose ends**: `cargo test` green, version 0.1.0 everywhere, internal-compiler-error panic hook with report link, clear one-line errors when `clang`/SDL2/libGL are missing, `j3c doctor`. Done when: `cargo test --release` passes; `PATH=/nonexistent j3c build` style check prints the friendly error.
4. [x] **Errors and literal-left**: report several type errors per check; `2 * far` typechecks like `far * 2`. Done when: tests for both pass, goldens unchanged.
5. [x] **Distribution + CI**: `tools/release/` scripts building a tarball and a `.deb` for `j3c`, `install.sh` (curl-able, to `~/.local/bin`), AUR `PKGBUILD`, GitHub Actions CI (cargo test, speculative 01–08, examples check) and tag release workflow, `CHANGELOG.md`, versioning policy. Done when: release script produces tarball + `.deb` locally; `install.sh` installs from a local tarball into a scratch prefix.
6. [x] **Human docs + community files**: README rewrite (real examples, quick start, hello world, install, platform matrix), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, issue/PR templates, mdBook Pages workflow, VS Code extension "j3c not found" message + license/publisher fields. Done when: every README command/path exists and the quick start runs headless.

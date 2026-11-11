Packages: make one, use one (path or git), j3.lock

A package is a J3 project other projects import. It is source only: no build scripts run when
it is fetched, and only the modules a program imports are compiled, for that program's target.

Make a package:

    j3c new hello --lib       hello/j3.toml with [lib], src/hello.j3, examples/demo, README.md
    j3c new --lib             in an existing project: adds [lib] (and missing files); moves nothing
    j3c check                 checks src/hello.j3 and what it imports (no `fn main` needed)
    j3c run examples/demo     the demo depends on the package by path ("../..")

    [package]
    name = "hello"            the name importers use; letters, digits, `_` (no `-`)
    version = "0.1.0"
    j3 = ">=0.1"              the oldest j3c it builds with (importers get a clear error)

    [lib]                     a package: no main; `j3c run` / `j3c build` refuse it

Modules: `use hello;` loads src/hello.j3, which says `module hello;`. `use hello.net.client;`
loads src/net/client.j3, which says `module hello.net.client;`. A package's own files import each
other the same way.

Use a package (j3.toml of the importing project):

    [dependencies]
    hello = "../hello"                                      a folder on disk (same as { path = "../hello" })
    hello = { git = "https://github.com/you/hello", tag = "v0.1.0" }
    hello = { git = "https://github.com/you/hello", rev = "c9b731e" }
    hello = { git = "https://github.com/you/hello", branch = "main" }
    hello = { git = "https://github.com/you/hello" }        the default branch
    hello = { git = "../hello", tag = "v0.1.0" }            a local repo works too (path is relative to this j3.toml)

or from the command line, which writes that line and fetches:

    j3c add hello --path ../hello
    j3c add hello --git https://github.com/you/hello --tag v0.1.0

Then `use hello;` in your code. A dep's own [dependencies] come along. Each name is one package
for the whole program: two deps that want `hello` from different places (or commits) is an error
that names both.

Git deps and j3.lock:

    The first check/run/build that needs a git dep runs the system `git` (clone, then checkout)
    into ~/.cache/j3/pkgs/<name>-<commit>/ (J3_PKG_DIR moves it) and writes j3.lock beside
    j3.toml: the exact commit of every git dep, transitive ones too. Commit j3.lock. With the
    lock and the cache present no git runs, so builds work offline and everyone gets the same
    code. A branch or default-branch dep stays at its locked commit until you update it.

    j3c fetch                 download every git dep now, write j3.lock, print where each one lives
    j3c update [name]         move git deps (or one) to the newest commit of their tag/branch
    Path deps are never locked: they are whatever is on disk.

    The LSP never runs git; a dep that is not downloaded says to run `j3c fetch`.

Publish a package: push the repo, then `git tag v0.1.0 && git push --tags`. Importers point at
the tag. There is no registry yet: share the git URL.

Limits:
    - Code only. A dep's assets/, .j3ui components, shaders, icons and scenes are not found for
      you, and its [link] libraries are not added to your build: list them in your own j3.toml.
    - No version ranges (`^0.3`): pin a tag, rev, or branch.
    - `j3 = ">=X.Y"` is the only compiler bound.
    - Hot reload (`j3c run --hot`) watches this project's files, not a dep's.

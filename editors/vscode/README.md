# J3 for VS Code

Language support for [J3](https://github.com/Jacoby-Y/j3), an ahead-of-time game language.

- Highlighting for `.j3`, `.j3s`, `.j3ui` and `j3.toml`.
- Completion, hover, go to definition, signature help, outline, import quick fixes and diagnostics, from `j3c lsp`.
- Format Document, and the **J3: Run**, **J3: Build** and **J3: Format** commands.

## Setup

The extension runs `j3c`, the J3 compiler. Install it first ([install steps](https://github.com/Jacoby-Y/j3#install)) and check it with `j3c doctor`.

If `j3c` is not on your `PATH`, set **`j3.j3cPath`** to the binary. Inside a clone of the J3 repository the extension uses the newest `j3c/target/{debug,release}/j3c` on its own.

Works in VS Code, Cursor and VSCodium.

## License

MIT OR Apache-2.0.

A headless program: reads `settings.toml` (or built-in defaults), prints a few values, changes
one and saves the file back. Run it twice: the second run reads what the first saved.

- `toml.parse` gives the same `Json` tree `j3.data.json` builds, so `get`, `path("a.b.0")`,
  `asStr`, `asInt`, `asBool`, `set` and `push` all work on it. `[[keys]]` is an array of tables.
- Reading never fails: a missing key is a null node (0, "", false). Errors carry a line.
- `toml.stringify` writes plain keys, then `[tables]` and `[[arrays of tables]]`; comments and
  layout of the original file are not kept.
- Unquoted words (`mode = fast`) read as strings, numbers keep their token in `s`.
  `toml.items` walks headers and keys line by line, duplicates included.
- Next: `j3c doc j3.data.toml`, `j3c doc j3.data.json --short`, `j3c doc j3.io writeFileAtomic`.

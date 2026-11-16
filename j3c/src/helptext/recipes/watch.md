Notice files changed outside the app (a file explorer's open folders, the file open in an editor)
without polling `io.stat` / `io.listDir`. `j3.io.watch` is inotify; `j3c doc j3.io.watch` has the rules.

- `Watcher.create(mem)`, then `w.add(path)`: `Result<int, IoError>`, the id each `Event` carries. A
  folder reports its direct entries (not recursive: add each open subfolder); a file reports itself.
- `w.poll(&evs)` each frame: appends `Event { id, kind, name }` and returns the count; never blocks,
  allocates nothing once `evs` has grown. Reset `evs.len = 0` after handling them.
- Kinds: folder `Create` / `Delete` / `Modify` / `MoveIn` / `MoveOut` with the entry `name`; the
  folder itself gone is `Delete` with `name` "". A file's `name` is "": an editor's
  "write temp + rename over" save is `Modify`, like an in-place write; delete then recreate is
  `Delete`, `Create` on the same id. `Overflow` (id -1): events were lost, rescan everything.
- `name` points into the watcher's buffer and is only valid until the next `poll`: copy what you keep.
- An idle `.j3ui` app sleeps between frames; a change wakes it like a worker hand-back, so the
  next frame's `poll` sees it. Poll from the draw system (an `always` system keeps the app awake).
- `w.remove(id)` (a gone folder or an unknown id is a no-op), `w.close()` at shutdown (stops the
  helper thread). Use a `Watcher` from the thread that made it. Linux x86-64 (inotify) and Windows
  (`ReadDirectoryChangesW`): elsewhere `add` returns `Err`.

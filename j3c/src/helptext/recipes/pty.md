Run a program on a pseudo-terminal and read what it drew: `pty.spawn` starts it with a size, `tryRead`
collects its bytes, a `Grid` plus `Parser` turn them into cells. Here `ls --color=always` runs and `dumpText` prints the screen as text.
`j3c doc term` is the overview; this is the emulator half without a window.

- Linux only. `spawn` is `Err` when the program cannot start.
- Read with `poll` plus `tryRead` and a deadline, never a bare loop: a program that never exits would hang you. `eof` is true once it closed its side.
- `p.write(bytes)` sends keys to it (queued, flushed on the next `tryRead`), `p.resize(cols, rows)` changes its size.
- `p.wait()` asks it to quit (SIGHUP, then SIGKILL after 500 ms) and returns its exit code. `j3.term.view.View` draws a Grid in a window (`examples/terminal`).

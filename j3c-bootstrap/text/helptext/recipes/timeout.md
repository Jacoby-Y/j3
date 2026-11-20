Run a command with a time limit and kill everything it started: `proc.startWith` with `StartOpts { group: true }`
puts the child in its own process group, `tryWait` plus `sys.sleepMs` polls against a `sys.monotonicMs` deadline,
and `Child.killTree` signals the whole group, so `sh -c "make test"` takes its compilers and test binaries with it.
Output goes to `run.log` (`stdoutFd`/`stderrFd`, opened with `O_APPEND`); rlimits cap CPU and memory.

- Linux and Android. On Windows `startWith` returns `Fail` for anything but `cwd`.
- `c.kill()` signals only the child: a grandchild (`sleep 30 &`) would keep running and keep the pipes open.
- SIGTERM first so the program can clean up, SIGKILL after a grace period, then `wait` to reap it.
- To keep the output in memory instead, leave `stdoutFd`/`stderrFd` at -1 and drain `c.tryRead(buf)` /
  `c.tryReadErr(buf)` in the poll loop: a child blocks once a full pipe (64 KB) is not read.
- `StartOpts` limits: `cpuSecs`, `memBytes` (address space), `fileBytes`, `maxProcs` (counts every process of
  the user). A limit that cannot be set makes `startWith` return `Fail`.

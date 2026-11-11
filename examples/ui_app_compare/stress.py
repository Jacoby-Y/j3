#!/usr/bin/env python3
"""Run each app's stress scene (STRESS=1) in its own Xvfb and measure FPS, CPU per frame and memory.

Each app warms up for 2 s, prints `mark`, counts frames for 8 s and prints `done frames=N`.
CPU is user + system time of every process the app started (renderer, GPU, web processes), so it
includes the software GL/Vulkan rendering Xvfb forces. Memory is the peak of the summed PSS of those
processes (shared pages split between them, not counted twice). The Xvfb server itself is left out.

Build the release apps first (./bench.sh). Usage: ./stress.py [app ...] [--runs N]
"""
import os, queue, re, signal, statistics, subprocess, sys, threading, time

ROOT = os.path.dirname(os.path.abspath(__file__))
# app: (working directory, command)
APPS = {
    "j3": ("j3", ["./target/todo"]),  # a J3 app loads assets (the font) relative to its package root
    # packaged Electron has no SUID sandbox helper set up; Chromium's sandbox does not change the work done per frame
    "electron": (".", ["electron/out/todo-linux-x64/todo", "--no-sandbox"]),
    "tauri": (".", ["tauri/target/release/todo-tauri"]),
    "wails": (".", ["wails/build/bin/todo"]),
    # Vello rasterizes in GPU compute shaders; on Xvfb's software Vulkan/GL it did not finish one frame in 20 s
    "dioxus": (".", ["dioxus/target/release/todo-dioxus"]),
}
TICK = os.sysconf("SC_CLK_TCK")


def tree(root):
    """Pids of root and all its descendants, minus the Xvfb server."""
    kids = {}
    for p in os.listdir("/proc"):
        if p.isdigit():
            try:
                stat = open(f"/proc/{p}/stat").read()
                ppid = int(stat.rsplit(")", 1)[1].split()[1])
                kids.setdefault(ppid, []).append(int(p))
            except (OSError, IndexError):
                pass
    out, todo = [], [root]
    while todo:
        p = todo.pop()
        try:
            if open(f"/proc/{p}/comm").read().strip() == "Xvfb":
                continue
        except OSError:
            continue
        out.append(p)
        todo += kids.get(p, [])
    return out


def cpu(pids):
    s = 0
    for p in pids:
        try:
            f = open(f"/proc/{p}/stat").read().rsplit(")", 1)[1].split()
            s += int(f[11]) + int(f[12])  # utime + stime
        except OSError:
            pass
    return s / TICK


def pss(pids):
    s = 0
    for p in pids:
        try:
            m = re.search(r"^Pss:\s+(\d+) kB", open(f"/proc/{p}/smaps_rollup").read(), re.M)
            s += int(m.group(1)) * 1024 if m else 0
        except OSError:
            pass
    return s


def run(app):
    env = dict(os.environ, STRESS="1", SDL_AUDIODRIVER="dummy", WEBKIT_DISABLE_DMABUF_RENDERER="1")
    env.pop("WAYLAND_DISPLAY", None)
    cwd, argv = APPS[app]
    cmd = ["xvfb-run", "-a", "-s", "-screen 0 1280x800x24", *argv]
    proc = subprocess.Popen(cmd, cwd=os.path.join(ROOT, cwd), env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            text=True, start_new_session=True)
    peak, stop = [0], threading.Event()

    def watch():
        while not stop.is_set():
            peak[0] = max(peak[0], pss(tree(proc.pid)))
            time.sleep(0.2)

    lines = queue.Queue()

    def read():
        for line in proc.stdout:
            lines.put(line.strip())

    threading.Thread(target=watch, daemon=True).start()
    threading.Thread(target=read, daemon=True).start()
    mark = done = None
    deadline = time.monotonic() + 60
    try:
        while not done and time.monotonic() < deadline:
            try:
                line = lines.get(timeout=1)
            except queue.Empty:
                continue
            if line == "mark":
                mark = (time.monotonic(), cpu(tree(proc.pid)))
            elif line.startswith("done frames=") and mark:
                done = (time.monotonic(), cpu(tree(proc.pid)), int(line.split("=")[1]))
    finally:
        stop.set()
        try:
            os.killpg(proc.pid, signal.SIGKILL)  # the app, its helpers and the Xvfb server
        except ProcessLookupError:
            pass
        proc.wait(timeout=20)
    if not done:
        return None
    secs, cpus, frames = done[0] - mark[0], done[1] - mark[1], done[2]
    return {"fps": frames / secs, "cpu_ms": cpus * 1000 / max(frames, 1), "mem": peak[0]}


def main():
    args = sys.argv[1:]
    runs = 3
    if "--runs" in args:
        i = args.index("--runs")
        runs = int(args[i + 1])
        del args[i:i + 2]
    apps = args or list(APPS)
    rows = []
    for app in apps:
        res = [r for r in (run(app) for _ in range(runs)) if r]
        if not res:
            print(f"{app}: no result", file=sys.stderr)
            continue
        med = lambda k: statistics.median(r[k] for r in res)
        rows.append((app, med("fps"), med("cpu_ms"), max(r["mem"] for r in res)))
        print(f"{app}: {rows[-1][1]:.1f} fps, {rows[-1][2]:.1f} CPU ms/frame, {rows[-1][3] / 2**20:.0f} MB", file=sys.stderr)
    print("| App | FPS | CPU per frame | Peak memory (PSS) |")
    print("|---|---:|---:|---:|")
    for app, fps, ms, mem in rows:
        print(f"| {app} | {fps:.1f} | {ms:.1f} ms | {mem / 2**20:.0f} MB |")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Run identical-setting benchmarks; keep raw logs, screenshots, and median results."""
import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("label")
parser.add_argument("--source", type=Path, default=ROOT)
parser.add_argument("--scenes", nargs="+", default=["start", "surface", "cave", "entrance", "factory"])
parser.add_argument("--samples", type=int, default=3)
parser.add_argument("--windowed", action="store_true")
parser.add_argument("--prepare-only", action="store_true")
parser.add_argument("--profile-stages", action="store_true")
parser.add_argument("--release", action="store_true", help="build with --release (opt s) instead of the project settings")
parser.add_argument("--renderer", choices=["software", "native"], default="software")
args = parser.parse_args()
def emit(path):
    return re.search(r"\[emit\]([^\[]*)", path.read_text()).group(1).strip()
assert emit(ROOT / "j3.toml") == emit(ROOT / "benchmarks/j3.toml"), "benchmark compiler settings differ"
out = ROOT / "target/benchmarks" / args.label
out.mkdir(parents=True, exist_ok=True)
project = out / "project"
project.mkdir(exist_ok=True)
if args.source.resolve() != project.resolve():
    shutil.copytree(args.source / "src", project / "src", dirs_exist_ok=True)
    shutil.copytree(args.source / "scripts", project / "scripts", dirs_exist_ok=True)
shutil.copyfile(ROOT / "benchmarks/j3.toml", project / "j3.toml")
# Keep the measurement harness identical even when measuring saved production code.
(project / "src/bench").mkdir(parents=True, exist_ok=True)
shutil.copyfile(ROOT / "src/bench/benchmark.j3", project / "src/bench/benchmark.j3")
# Stage timers are built into the game; this file switches them on for the benchmark build.
profile_file = project / "target/benchmark-profile.txt"
profile_file.parent.mkdir(exist_ok=True)
if args.profile_stages:
    profile_file.write_text("1")
elif profile_file.exists():
    profile_file.unlink()
if not (project / "assets").exists():
    (project / "assets").symlink_to(ROOT / "assets", target_is_directory=True)
if args.prepare_only:
    print(project)
    raise SystemExit(0)
results = json.loads((out / "results.json").read_text()) if (out / "results.json").exists() else {}
for scene in args.scenes:
    (project / "target").mkdir(exist_ok=True)
    (project / "target/benchmark-case.txt").write_text(scene)
    samples = []
    for sample in range(args.samples):
        cmd = ["/usr/bin/time", "-f", "PEAK_RSS_KB=%M", "timeout", "300s", "j3c", "run", str(project), "--frames", "180" if scene == "factory" else "600"]
        if args.release:
            cmd += ["--release"]
        if not args.windowed:
            cmd += ["--headless"]
        if sample == 0:
            cmd += ["--screenshot", str(out / f"{scene}.png")]
        env = dict(os.environ)
        if args.renderer == "software":
            env.update(__GLX_VENDOR_LIBRARY_NAME="mesa", LIBGL_ALWAYS_SOFTWARE="1")
        run = subprocess.run(cmd, env=env, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / f"{scene}-{sample + 1}.log").write_text(run.stdout)
        if run.returncode:
            raise SystemExit(f"{scene} failed ({run.returncode}):\n{run.stdout[-2000:]}")
        stats = re.search(r"J3STATS .*", run.stdout)
        if not stats:
            raise SystemExit(f"missing renderer stats for {scene}")
        record = {k: float(v) for k, v in re.findall(r"(\w+)=([-\d.]+)", stats.group())}
        record["renderer"] = stats.group().split(" gl=", 1)[1]
        for key, value in re.findall(r"(startup_ms|edit_ms|update_ms|draw_submit_ms|furnace_ms|transport_ms|drill_ms)=([-\d.]+)", run.stdout):
            record[key] = float(value)
        record["peak_rss_kb"] = int(re.search(r"PEAK_RSS_KB=(\d+)", run.stdout).group(1))
        required = {"update_ms", "draw_submit_ms", "edit_ms", "startup_ms", "frame"}
        if not required <= record.keys():
            raise SystemExit(f"missing metrics for {scene}: {required - record.keys()}")
        samples.append(record)
        print(f"{args.label} {scene} {sample+1}: {record}", flush=True)
    if len({s["renderer"] for s in samples}) != 1:
        raise SystemExit("renderer changed between samples")
    results[scene] = {"samples": samples, "median": {key: statistics.median(s[key] for s in samples) for key in samples[0] if key != "renderer"}, "renderer": samples[0]["renderer"]}
    (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")

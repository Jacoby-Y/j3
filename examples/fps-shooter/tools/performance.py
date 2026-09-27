#!/usr/bin/env python3
"""Run the built shooter's opt-in workload and summarize buffered frame samples.

python3 tools/performance.py --quick
python3 tools/performance.py --repeats 3
python3 tools/performance.py --window --scenario populated --repeats 3
Build first with j3c build examples/fps-shooter. Outputs go under target/performance.
"""
import argparse
import csv
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def percentile(values, percent):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * percent / 100) - 1)]


def summarize(path):
    lines = path.read_text().splitlines()
    start = next((i for i, line in enumerate(lines) if line.startswith('FPSPERF ')), None)
    if start is None:
        raise RuntimeError(f'No samples in {path}; inspect the log')
    rows = list(csv.DictReader([lines[start].removeprefix('FPSPERF ')] + [x for x in lines[start + 1:] if x and x[0].isdigit()]))
    if not rows:
        raise RuntimeError('Empty capture')
    data = {k: [int(row[k]) for row in rows] for k in rows[0]}
    frames = data['frame_us']
    events = [int(row['frame_us']) for row in rows if row['event'] == '1']
    result = {
        'samples': len(rows), 'mean_ms': statistics.mean(frames) / 1000,
        'p95_ms': percentile(frames, 95) / 1000, 'p99_ms': percentile(frames, 99) / 1000,
        'worst_ms': max(frames) / 1000, 'kill_worst_ms': max(events, default=0) / 1000,
        'kill_events': len(events),
        'mean': {(k.removesuffix('_us') + '_ms' if k.endswith('_us') else k): (statistics.mean([x for x in v if x >= 0]) / (1000 if k.endswith('_us') else 1) if any(x >= 0 for x in v) else None) for k, v in data.items()},
        'gpu_samples': sum(x >= 0 for x in data['shadow_us']),
        'max_living': max(data['living']), 'max_active': max(data['active']),
    }
    result['passed'] = result['p95_ms'] <= 10 and result['p99_ms'] <= 16.7 and result['kill_worst_ms'] <= 25
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quick', action='store_true', help='1 s warm-up + 3 s capture; smoke test only')
    parser.add_argument('--window', action='store_true', help='normal window presentation; otherwise offscreen without glFinish')
    parser.add_argument('--scenario', choices=['opening', 'populated', 'pack'])
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    out = ROOT / 'target/performance'
    out.mkdir(parents=True, exist_ok=True)
    metadata = {'platform': platform.platform(), 'resolution': '1280x720', 'quick': args.quick,
                'presentation': 'window' if args.window else 'offscreen, no glFinish',
                'warmup_seconds': 1 if args.quick else 10, 'capture_seconds': 3 if args.quick else 60,
                'build': (ROOT / 'j3.toml').read_text(), 'results': []}
    try:
        metadata['gpu'] = subprocess.check_output(['nvidia-smi', '--query-gpu=name,driver_version', '--format=csv,noheader'], text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        metadata['gpu'] = 'See F3 renderer name'
    for scenario in ([args.scenario] if args.scenario else ['opening', 'populated', 'pack']):
        for run in range(args.repeats):
            env = os.environ.copy()
            for key in ['J3_FRAMES', 'J3_SCREENSHOT', 'J3_FPS_QUICK']:
                env.pop(key, None)
            env.update(J3_FPS_BENCH=scenario, SDL_AUDIODRIVER='dummy', __GL_SYNC_TO_VBLANK='0')
            if not args.window:
                env['SDL_VIDEODRIVER'] = 'offscreen'
            if args.quick:
                env['J3_FPS_QUICK'] = '1'
            log = out / f'{scenario}-{run + 1}{"-quick" if args.quick else ""}{"-window" if args.window else ""}.csv'
            with log.open('w') as stream:
                subprocess.run([str(ROOT / 'target/fps_shooter')], cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT,
                               check=True, timeout=30 if args.quick else 120)
            result = summarize(log)
            result.update(scenario=scenario, run=run + 1, log=str(log))
            metadata['results'].append(result)
            print(json.dumps(result), flush=True)
            (out / f'summary{"-quick" if args.quick else ""}{"-window" if args.window else ""}.json').write_text(json.dumps(metadata, indent=2) + '\n')
    if not all(r['passed'] for r in metadata['results']):
        raise SystemExit(1)


if __name__ == '__main__':
    main()

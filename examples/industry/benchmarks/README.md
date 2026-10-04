Run from the industry root:

```sh
python3 benchmarks/run.py after
python3 benchmarks/run.py gpu-after --renderer native --prepare-only
python3 benchmarks/run.py release-after --release
```

Each run snapshots the current source/scripts under `target/benchmarks/<label>/project`, checks that benchmark and production compiler settings match, and runs three samples per scene (600 frames, or 180 for the expensive factory case). It saves raw logs, one screenshot per scene, peak RSS, and median metrics in `results.json`. Each game invocation has a five-minute timeout. `--scenes start surface cave entrance factory` selects cases; `--source <snapshot-project>` measures saved source. Labels can be resumed for selected scenes.

`--profile-stages` switches on the game's own per-stage timers (a `target/benchmark-profile.txt` file in the snapshot) and reports furnace, belt and drill averages after warmup in a 180-frame factory run. Profile these separately from acceptance samples so timer overhead does not affect the frame-time comparison. `--release` builds with `j3c run --release` (opt s) instead of the project settings; compare release runs only with release runs.

The harness is `src/bench/benchmark.j3`; it runs the production update and draw paths.

The default explicitly uses llvmpipe for consistent software-renderer comparisons. Native mode uses the environment's available driver; `--windowed` enables bounded visible runs when headless selects software rendering. Verify the renderer in the logs before treating these as hardware results. Compare only runs on the same renderer and resolution. Shadow and camera pass timings come from `J3STATS`; draw submission and update timings measure CPU work. RSS includes the launcher/compiler and game; use the median to reduce first-build overhead. FPS/frame times exclude the runner's warmup frames.

In this agent environment, Python-launched runs selected llvmpipe while direct compiler runs selected the RTX 5060 Ti. To measure a prepared snapshot directly, select a case through its file and repeat the command three times, saving each output. Use 180 frames for `factory`, 600 for the other cases. Direct hardware samples omit RSS measurement because wrapping the command in `/usr/bin/time` also selected llvmpipe here.

```sh
mkdir -p target/benchmarks/gpu-after/project/target
printf %s surface > target/benchmarks/gpu-after/project/target/benchmark-case.txt
timeout 60s j3c run target/benchmarks/gpu-after/project --frames 600 --headless
```

All cases use seed 42, 1280×720, unchanged production compiler settings, clip distance, lighting, and LOD thresholds. The harness calls the production update and draw functions. It supplies one 1/60-second simulation step per frame to keep factory states comparable across rendering speeds; production continues using its existing variable timestep. Updates and draw submission are measured after 60 warmup frames.

Cases cover the starting chunk, all 81 chunks from the surface, an underground cave, a rare surface entrance, and a factory. The factory adds exactly 1,024 loaded conveyors, 32 drills facing copper blocks, 32 fueled furnaces, 16 containers, and eight markets. Lines use the real transfer/processing rules, including blocked output. Starting queues and clocks are recreated for every sample.

Correctness checks run with `j3c check`, `j3c check tests`, and `timeout 60s j3c run tests`. They compare exact model face coverage, colors, emission, rotations, animated bounds, and winding; verify separate cargo/camera/shadow culling; and compare section meshes with whole-chunk reference faces including original AO data, vertical seams, holes, top sections, and complete machine edit footprints. Shadow batching tests verify identical cached geometry after edits, empty-cache release, and fallback to individual section draws when a chunk is partly visible.

Measured results and tradeoffs are in [PERFORMANCE.md](PERFORMANCE.md).

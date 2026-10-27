Plan 023 chunk 3b visual fixture: real DOM title tooltips and color panels. From the repo root:

```
J3C=$PWD/j3c/target/release/j3c timeout 180 speculative/22_ui_popups/tools/golden.sh
```

The script builds and runs only offscreen, captures frame 60, and compares six states exactly: short/default,
wrapped/default and themed tooltips, default/themed color panels, and a themed color panel beside a tooltip.
`--update` replaces the checked-in baselines. Text uses the existing big_ui_test font asset. The mechanism,
input/focus/modal behavior, steady costs and reopening are checked by `j3c/stdlib/tests/overlay_test.j3` and
`input_test.j3`; the previous select goldens remain in `speculative/21_ui_select`.

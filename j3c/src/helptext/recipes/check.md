Each step catches something the others don't. From the project root:

    j3c check                     J3 type errors (file:line), missing libraries, and files
                                  that load calls name; wrong script props against the
                                  scene line. Does not run anything.
    j3c -e scene check --all      every scene: broken parents, missing mesh, material,
                                  texture, and scene files, unknown scripts, placement cycles
    j3c -e render --view iso --out shot.png
                                  the scene as the game draws it, without the game's code
    j3c run --frames 120 --screenshot shot.png --headless
                                  the real game for 2 seconds with no window; the last
                                  frame is saved and output ends with a J3STATS line.
                                  THE way to test a game: never run one without --headless
    J3_CAPTURE=out.png:30 J3_FIXED_DT=0.016 ./target/<name>   (with SDL_VIDEODRIVER=offscreen
                                  SDL_AUDIODRIVER=dummy) then `j3c -e imgdiff golden.png out.png`
                                  is a golden test: `j3c doc testing`

Paths: `--out` must be inside the project; `--screenshot` is relative to where you ran
j3c. The game's stdout and stderr come back to you. The editor's Play writes build errors
to target/j3-editor/build.log. The MCP server (`j3c -e mcp`) has the same steps as the
check, render, and play tools.

Start from the task. `j3c doc recipes <name>` prints a complete, checked program.
    set up this machine              `j3c doctor`
    2D game in a window              recipes app2d     then j3.app, j3.game, j3.ui, `j3c doc j3ui-immediate`
    menus, HUDs, panels styled with CSS recipes components then `j3c doc j3ui`
    3D game from editor scenes       recipes scene3d   then j3.scene3d, j3.gfx, `j3c doc editor`
    first-person 3D starter          `j3c new <name> --template fps`, then j3.scene3d.fpwalk
    sprites and textures, no window  recipes canvas    then j3.gfx.canvas
    shapes, text, HUDs, debug draw   recipes paint     then j3.gfx.paint
    video settings (mode, resolution, vsync)  recipes display  then j3.game.display
    per-object logic in .j3s         recipes script    then `j3c doc lang` (Scripts), j3.scene3d.scripts
    mods loaded at runtime (.j2)     recipes j2        then `j3c doc j2`
    meshes built in code             recipes meshgen   then j3.gfx.meshgen
    heavy work off the frame thread  recipes threads   then `j3c doc threads`, j3.thread
    call a web API (https, JSON)     recipes http      then j3.net.http, j3.data.json, examples/openrouter-chat
    run another program (curl, git)  j3.proc           (`proc.run(argv, stdin, &arena)`: exit code, stdout, stderr)
    materials and shader effects     recipes material  then j3.scene3d.fx, `j3c doc editor`
    traced mirrors, glass, shadows   recipes urp       then `j3c doc urp`
    check and look at what you built recipes check
    run, screenshot, golden-diff     `j3c doc testing` (headless: never open a window)
    CSS support for .j3ui            `j3c doc css`
    ship it: zip, .deb, icon         `j3c doc ship`
    saves, assets, ECS, physics      j3.data.save, j3.data.assets, j3.game.ecs, j3.game.physics
Find a name without knowing its module: `j3c doc search <word>`, or `j3c doc <name>`.
A module's names on one screen: `j3c doc <module> --short`.


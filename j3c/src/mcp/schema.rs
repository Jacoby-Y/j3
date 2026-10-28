use super::*;

/// Tools that change a scene; their replies say whether the change went to the editor or the file.
pub(super) const EDITS: &[&str] = &["scene_add", "scene_add_many", "scene_set", "scene_delete", "scene_apply", "scene_fmt", "scene_split", "scene_inline", "place"];

/// The CLI command doing each tool's work; "" when only MCP has it.
pub(super) const CLI_SAME: &[(&str, &str)] = &[
    ("scene_list", "j3c -e scene list"),
    ("scene_get", "j3c -e scene get <object>|@scene"),
    ("scene_add", "j3c -e scene add <kind> key=value ..."),
    ("scene_add_many", ""),
    ("scene_apply", "j3c -e scene apply FILE|-"),
    ("scene_split", "j3c -e scene split <object> ... out=FILE"),
    ("scene_inline", "j3c -e scene inline <instance>"),
    ("scene_fmt", "j3c -e scene fmt"),
    ("scene_set", "j3c -e scene set <object> key=value ..."),
    ("scene_delete", "j3c -e scene delete <object> ..."),
    ("place", "j3c -e scene ground|ontop|scatter|ring|stairs|spiral|wall ... (grid is MCP only)"),
    ("undo", "j3c -e scene undo"),
    ("render", "j3c -e render"),
    ("check", "j3c -e scene check --all, then j3c check"),
    ("play", "j3c run --frames N --screenshot F.png --headless"),
    ("console", ""),
    ("import", ""),
    ("asset_search", ""),
    ("asset_online_search", ""),
    ("asset_fetch", ""),
    ("gen", "j3c -e gen [--force]"),
    ("scene_stats", "j3c -e scene stats [--all]"),
    ("scene_diff", "j3c -e scene diff [OLD]"),
];

/// `j3c -e mcp --tools`: every tool's arguments, from the same definitions as `tools/list`.
pub(super) fn tool_reference() -> String {
    let mut out = String::from("MCP tools (`j3c -e mcp`), from the definitions `tools/list` sends. * marks a required argument.\n");
    let Json::Arr(list) = tools() else { return out };
    for tool in &list {
        let name = tool.get("name").and_then(Json::as_str).unwrap_or("");
        let title = tool.get("title").and_then(Json::as_str).unwrap_or("");
        out.push_str(&format!("\n{name} — {title}\n"));
        let cli = CLI_SAME.iter().find(|(n, _)| *n == name).map(|(_, c)| *c).unwrap_or("");
        out.push_str(&format!("    CLI: {}\n", if cli.is_empty() { "none; MCP only" } else { cli }));
        if let Some(d) = tool.get("description").and_then(Json::as_str) {
            out.push_str(&format!("    {d}\n"));
        }
        if let Some(schema) = tool.get("inputSchema") {
            push_args(&mut out, schema, "    ");
        }
    }
    out
}

pub(super) fn push_args(out: &mut String, schema: &Json, indent: &str) {
    let Some(Json::Obj(props)) = schema.get("properties") else { return };
    let required: Vec<&str> = match schema.get("required") {
        Some(Json::Arr(r)) => r.iter().filter_map(Json::as_str).collect(),
        _ => Vec::new(),
    };
    for (key, prop) in props {
        let mut line = format!("{indent}{key}{}: {}", if required.contains(&key.as_str()) { "*" } else { "" }, type_text(prop));
        if let Some(Json::Arr(e)) = prop.get("enum") {
            let names: Vec<&str> = e.iter().filter_map(Json::as_str).collect();
            line.push_str(&format!(" ({})", names.join("|")));
        }
        if let Some(d) = prop.get("default") {
            line.push_str(&format!(" = {}", d.stringify()));
        }
        if let Some(d) = prop.get("description").and_then(Json::as_str) {
            line.push_str(&format!(" — {d}"));
        }
        out.push_str(&line);
        out.push('\n');
        let item = prop.get("items").unwrap_or(prop);
        if item.get("properties").is_some() {
            push_args(out, item, &format!("{indent}    "));
        }
    }
}

pub(super) fn type_text(prop: &Json) -> String {
    let ty = prop.get("type").and_then(Json::as_str).unwrap_or("any");
    match (ty, prop.get("items").and_then(|i| i.get("type")).and_then(Json::as_str)) {
        ("array", Some(inner)) => format!("array of {inner}"),
        _ => ty.to_string(),
    }
}

pub(super) const TOOL_NAMES: &[&str] = &[
    "scene_list", "scene_get", "scene_add", "scene_add_many", "scene_apply", "scene_split", "scene_inline", "scene_fmt", "scene_set", "scene_delete", "place", "undo", "render", "check", "play", "console", "import",
    "asset_search", "asset_online_search", "asset_fetch", "gen", "scene_stats", "scene_diff",
];

/// Tool definitions, as JSON text so they read like the schema they are.
pub(super) fn tools() -> Json {
    json::parse(TOOLS).expect("tool definitions are valid JSON")
}

pub(super) const TOOLS: &str = r#"[
{"name":"scene_list","title":"List scene objects",
 "description":"Start here to learn what a scene holds (filter with a glob; group_by prefix counts names like DecoCorn). List every object in a scene: #id, Parent/Child path, kind (group, mesh, camera, light, instance), asset, world position, rotation (degrees), scale, script. Objects placed by an instance (another scene file) are not listed; edit that scene instead.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"Scene file, e.g. scenes/main.j3scene. Default: the scene in j3.editor.toml."},
   "filter":{"type":"string","description":"Only objects whose id, path, or kind contains this text; with * or ? a glob on name or path (DecoCorn*)."},
   "group_by":{"type":"string","enum":["prefix"],"description":"Count objects by name with trailing digits dropped instead of listing them."},
   "tree":{"type":"boolean","description":"Print the objects as the scene file writes them: one line each, indent is depth, `Name kind key=value #id`. scene_apply takes those lines back."}}},
 "annotations":{"readOnlyHint":true}},
{"name":"scene_get","title":"Get an object",
 "description":"One object as the scene file writes it (its line, then its children's, indented; scene_apply takes the lines back), then all its fields including its script's props (param.<prop>). Pass node \"@scene\" for scene lighting (sun, sky, fog, shadows, URP).",
 "inputSchema":{"type":"object","required":["node"],"properties":{
   "node":{"type":"string","description":"Name, Parent/Child path, #id, or @scene."},
   "scene":{"type":"string"}}},
 "annotations":{"readOnlyHint":true}},
{"name":"scene_add","title":"Add an object",
 "description":"Add one object and return its #id and path. For more than a few, use scene_add_many, or place for rings, walls, stairs, grids and scatter. Shapes (cube, sphere, cylinder, cone, capsule, torus, quad, plane, sky) are meshes with that built-in asset. Top-level meshes rest on y=0 unless pos is given; names are made unique. For a mesh file use kind mesh with fields.asset; to place another scene use kind instance with fields.scene.",
 "inputSchema":{"type":"object","required":["kind"],"properties":{
   "kind":{"type":"string","enum":["group","mesh","camera","light","instance","cube","sphere","cylinder","cone","capsule","torus","quad","plane","sky"]},
   "name":{"type":"string"},
   "parent":{"type":"string","description":"Parent object (name, path, or #id); default top level."},
   "fields":{"type":"object","description":"Same keys as scene_set, e.g. {\"pos\":[0,1,0],\"scale\":2,\"material\":\"assets/materials/wood.j3mat\"}."},
   "scene":{"type":"string"}}}},
{"name":"scene_add_many","title":"Add several objects",
 "description":"Use this (or place) instead of many scene_add calls. Add several objects in one step (one file write, or one undo step in the editor). Each item is a scene_add argument object. All or nothing: if one fails, none are added.",
 "inputSchema":{"type":"object","required":["adds"],"properties":{
   "adds":{"type":"array","items":{"type":"object","required":["kind"],"properties":{
     "kind":{"type":"string"},"name":{"type":"string"},"parent":{"type":"string"},"fields":{"type":"object"}}}},
   "scene":{"type":"string"}}}},
{"name":"scene_apply","title":"Write objects as scene lines",
 "description":"Add or replace objects written the way the scene file writes them, which is what scene_get and scene_list tree print: `Name kind key=value ... #id`, two spaces of indent per level of nesting, `//` lines above an object are its note. Kinds: group, mesh [path], place <scene>, light, camera, or a shape (cube sphere cylinder cone capsule torus quad plane sky). Keys: pos rot (degrees) scale (one number or three) tint color range fov near far material (grain means assets/materials/grain.j3mat) texture fx alpha opacity doubleSided script tags param.<prop>=value prop.<name>=value. Numbers with a . are floats, \"text\" is a string, #7 is an object. A line whose #id is an object in the scene replaces that object: fields it leaves out go back to their defaults (use scene_set to change one field). A line without #id, or with an unused one, adds an object with a new id; objects it nests keep their place under it, and #refs between new lines follow. Top-level lines go under parent. One undo step; all or nothing. Use it to add many varied objects at once, copy a subtree (get it, edit the lines, drop the ids), or rewrite one object whole.",
 "inputSchema":{"type":"object","required":["lines"],"properties":{
   "lines":{"type":"string","description":"The object lines, separated by newlines."},
   "parent":{"type":"string","description":"Object to put the top-level lines under (name, path, or #id); default top level."},
   "scene":{"type":"string"}}}},
{"name":"scene_split","title":"Move objects into their own scene file",
 "description":"Move top-level objects, and everything under them, into a new scene file out (project-relative, must not exist yet), keeping their params. With center the new file is centred on the objects' middle (x, z); with instance a placed copy of the new file is left where they were, so the world looks the same and the part can be edited or reused on its own. Use it to break a big scene into regions or to make a reusable piece (a house, a tree); a game can load several files into one world with scene3d.loadParts. Objects inside another object must be moved to the top level first (scene_set parent \"\").",
 "inputSchema":{"type":"object","required":["out"],"properties":{
   "node":{"type":"string","description":"Object to move (name, path, #id, or a glob)."},
   "nodes":{"type":"array","items":{"type":"string"}},
   "out":{"type":"string","description":"New scene file, e.g. scenes/biomes/forest.j3scene."},
   "instance":{"type":"boolean","default":false},
   "center":{"type":"boolean","default":false},
   "scene":{"type":"string"}}}},
{"name":"scene_inline","title":"Turn a placed scene into real objects",
 "description":"Replace a placed scene (an instance object) with real copies of its objects, so they can be edited in this scene. Per-copy values and the placement's tint are kept; scenes the placed scene places stay placed. The instance becomes a group.",
 "inputSchema":{"type":"object","required":["node"],"properties":{
   "node":{"type":"string","description":"The instance object."},
   "scene":{"type":"string"}}}},
{"name":"scene_fmt","title":"Rewrite a scene file in the canonical layout",
 "description":"Rewrite the scene file in the canonical layout: tree order, defaults left out, numbers trimmed. Only needed after the file was edited by hand; the tools always write it this way. Fails (changing nothing) when the file has a line it can't read, naming the line.",
 "inputSchema":{"type":"object","properties":{"scene":{"type":"string"}}}},
{"name":"scene_set","title":"Change an object",
 "description":"Set fields on an object (all or nothing). node may be a glob (DecoCorn*, *Tree?) to set every matching object; preview with scene_list filter. Keys: name; pos [x,y,z] (local to parent), x, y, z; rot [x,y,z] degrees; scale [x,y,z] or one number; parent (\"\" for top level; keeps local transform); asset (mesh file or builtin:cube|sphere|cylinder|cone|capsule|torus|quad|plane|sky); texture; material (.j3mat); scene (instance's scene file); script (a .j3s path, \"\" for none); tags (comma-separated, e.g. \"nocollide,launch=30\"; the game reads them with scene3d.hasTag/tagValue); param.<prop> (a prop of the object's script, typed by its props block); tint [r,g,b] or one number (meshes and placed scenes; multiplies color, 1 unchanged, above 1 allowed, e.g. [0.7,1,1.8] turns green teal); color [r,g,b] 0-1 and range (lights); fov, near, far (cameras); effects (cameras: kind=camera .j3fx files run in order, \"a.j3fx|b.j3fx\"; \"\" for none) and fx.<file stem>.<param> (their params); alpha opaque|cutout|blend; opacity; doubleSided; prop.<name> (on an instance: a prop its scene declares; \"\" goes back to the default). With node \"@scene\": prop.<name> = \"<type>:<default>\" declares a prop on this scene (type int, float, text, bool, material, mesh, scene; \"\" removes it), and sun [x,y,z] (direction toward the sun), sunColor, sky, fogStart, fogEnd, shadows, shadowSize, urp, urpBounces, urpSamples, urpShadows, urpShadowNear (meters traced sun shadows reach; the shadow map covers the rest; 0 no limit). save: true also saves the scene in the open editor.",
 "inputSchema":{"type":"object","required":["node"],"properties":{
   "node":{"type":"string","description":"Name, Parent/Child path, #id, or @scene."},
   "fields":{"type":"object"},
   "save":{"type":"boolean","description":"With the editor open, save the scene after the change (without it, edits stay unsaved in the editor)."},
   "scene":{"type":"string"}}}},
{"name":"scene_delete","title":"Delete objects",
 "description":"Delete objects and everything under them. A node may be a glob (DecoCorn*) for every match; preview with scene_list filter.",
 "inputSchema":{"type":"object","properties":{
   "node":{"type":"string"},
   "nodes":{"type":"array","items":{"type":"string"}},
   "scene":{"type":"string"}}},
 "annotations":{"destructiveHint":true}},
{"name":"place","title":"Place objects",
 "description":"Use for anything repeated or resting on something, instead of computing coordinates by hand. Placement without coordinates. ground: drop or lift node so its lowest point rests on the surface below it (or on target only; y=0 if nothing is below). ontop: center node over target and rest it on target's top. scatter: add count copies of what (a shape like cube or cylinder, a .j3mesh or builtin: asset, or a .j3scene placed as instances) at random spots in area [x0,z0,x1,z1] (in parent's space) under a new Scatter group, each grounded, with random yaw and a scale between scale[0] and scale[1]; the same seed gives the same layout. grid: add pieces (walls, floors, roof parts) on a grid in one step: each piece's what (a .j3scene, .j3mesh, builtin: asset, or shape) goes at [x*cell, floor*height, z*cell] turned turn*90 degrees about y, under parent. ring, stairs, spiral, wall build a shape from copies of what (default cube; size is scale, builtin shapes are 1 m) in one step, named name, under parent; angles are degrees. ring: count pieces at radius around center [x,z] (arc [a0,a1] for part of a circle), bottoms at y; face tangent (default: slabs whose local x points outward, widened so the ring has no slits when size z is 0), out or in (local +z faces out or in). stairs: steps from [x,y,z] along dir (x, -x, z, -z), each rise higher and run long, width wide; solid fills to the floor. spiral: floating slabs at radius around center, each turn degrees further and rise higher (keep rise*360/turn above about 2.5 m headroom), thickness thick, width radial, depth tangential. wall: a run from [x,z] to [x,z], height high and thickness thick, with door [width,height,at] cut at meters along it (lintel over it).",
 "inputSchema":{"type":"object","required":["op"],"properties":{
   "op":{"type":"string","enum":["ground","ontop","scatter","grid","ring","stairs","spiral","wall"]},
   "node":{"type":"string","description":"Object to move (ground, ontop)."},
   "target":{"type":"string","description":"ontop: object to put node on. ground: rest only on this object."},
   "what":{"type":"string","description":"scatter: shape, mesh asset, or scene file."},
   "area":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"scatter: x0, z0, x1, z1."},
   "count":{"type":"integer","default":10},
   "seed":{"type":"integer","default":1},
   "scale":{"type":"array","items":{"type":"number"},"minItems":1,"maxItems":2},
   "yaw":{"type":"boolean","default":true},
   "spacing":{"type":"number","description":"scatter: least distance between copies."},
   "parent":{"type":"string"},
   "name":{"type":"string","description":"scatter: base name of the copies."},
   "ground":{"type":"boolean","default":true},
   "cell":{"type":"number","default":1,"description":"grid: cell size."},
   "height":{"type":"number","description":"grid: floor height (default cell). wall: its height (default 3)."},
   "center":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"ring, spiral: [x,z]."},
   "radius":{"type":"number","description":"ring, spiral."},
   "y":{"type":"number","description":"ring, spiral, wall: base height."},
   "size":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3,"description":"ring: piece scale [radial, height, tangential]; tangential 0 fits the ring."},
   "face":{"type":"string","enum":["tangent","out","in"]},
   "arc":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"ring: from and to angle, degrees."},
   "from":{"type":"array","items":{"type":"number"},"description":"stairs: [x,y,z] of the bottom edge's middle. wall: [x,z]."},
   "to":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"wall: [x,z]."},
   "dir":{"type":"string","enum":["x","-x","z","-z"]},
   "steps":{"type":"integer"},
   "rise":{"type":"number","default":0.4},
   "run":{"type":"number","default":1},
   "width":{"type":"number","description":"stairs (default 2.5), spiral slab radial size (default 2)."},
   "solid":{"type":"boolean","default":true},
   "turn":{"type":"number","default":20,"description":"spiral: degrees per step."},
   "start":{"type":"number","default":0,"description":"spiral: first step's angle, degrees."},
   "thickness":{"type":"number","description":"spiral slab (default 0.6), wall (default 1.5)."},
   "depth":{"type":"number","default":1.2,"description":"spiral: slab size along the turn."},
   "door":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3,"description":"wall: [width, height, at]."},
   "pieces":{"type":"array","maxItems":2000,"items":{"type":"object","required":["what"],"properties":{
     "what":{"type":"string"},"x":{"type":"number"},"z":{"type":"number"},"floor":{"type":"number"},
     "turn":{"type":"integer","description":"Quarter turns about y."},"name":{"type":"string"}}}},
   "scene":{"type":"string"}}}},
{"name":"undo","title":"Undo the last change",
 "description":"Put a scene file back as it was before the last change made through these tools (keeps 20 per scene). Only when the editor is closed; with it open, the person uses Ctrl+Z.",
 "inputSchema":{"type":"object","properties":{"scene":{"type":"string"}}},
 "annotations":{"destructiveHint":true}},
{"name":"render","title":"Render the scene",
 "description":"Look at what you built; do this after every few edits, and from where the player would stand (at) for rooms, doors, stairs and jumps. Draw the scene as the game would (lights, shadows, sky, URP) and return the image. Default: through the main camera, or an overview if there is none. view frames all objects from a side; focus frames one object (with view's side); select outlines an object in orange. at looks from any spot (no camera object needed): yaw and pitch are degrees like a camera's rot y and x (yaw 0 looks toward -z, 90 toward -x, 180 toward +z, -90 toward +x; pitch above 0 looks up). views puts several such views in one image, two per row, width x height each. hide leaves out objects by name glob (Roof*) to see inside. ambient lifts dark interiors (default 0.25 with at/views). ortho with view top gives a true plan. With the editor open it renders the live, unsaved scene.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string"},
   "camera":{"type":"string","description":"Camera object to look through."},
   "view":{"type":"string","enum":["iso","front","back","side","top"]},
   "focus":{"type":"string","description":"Object to frame."},
   "select":{"type":"string","description":"Object to outline."},
   "at":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3,"description":"Eye position [x,y,z] in world space."},
   "yaw":{"type":"number","default":0,"description":"With at: degrees; 0 looks toward -z, 90 toward -x."},
   "pitch":{"type":"number","default":0,"description":"With at: degrees; above 0 looks up."},
   "views":{"type":"array","maxItems":16,"items":{"type":"array","items":{"type":"number"},"minItems":5,"maxItems":5},"description":"Several [x,y,z,yaw,pitch] views in one image, left to right, top to bottom."},
   "fov":{"type":"number","description":"Vertical field of view in degrees."},
   "hide":{"type":"string","description":"Comma-separated name globs to leave out with their children, e.g. \"Roof*,Ceiling\"."},
   "ambient":{"type":"number","description":"0 to 1: white mixed into the ambient light; render only."},
   "ortho":{"type":"boolean","description":"view/focus without perspective."},
   "width":{"type":"integer","default":640},
   "height":{"type":"integer","default":360}}},
 "annotations":{"readOnlyHint":true}},
{"name":"check","title":"Check scenes and code",
 "description":"Run before play and before saying you are done; it names the object at fault, and runs the project's own rules (tools/check.j3s) too. Find problems before playing: broken parents, missing mesh/material/texture/scene files, placement cycles, missing scripts, object fields pointing at deleted objects, same-named siblings, and J3 compile errors (with file and line).",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"One scene; default every scene under scenes/."},
   "code":{"type":"boolean","default":true,"description":"Also typecheck the project's code."}}},
 "annotations":{"readOnlyHint":true}},
{"name":"play","title":"Play the game",
 "description":"Build the game and run it without a window for some frames (scripts run), then return the last frame as an image with the game's output. For a 3D game the output ends with a J3STATS line: average fps, draw calls, triangles, and CPU/GPU milliseconds. Build errors come back instead of an image.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"Scene to start in; default the project's main scene."},
   "frames":{"type":"integer","default":120,"description":"Frames to run (60 per second)."},
   "timeout":{"type":"integer","default":60,"description":"Seconds before the run is stopped."}}}},
{"name":"console","title":"Game output",
 "description":"Recent game and build output: the editor's console while the editor is open, else the last play run.",
 "inputSchema":{"type":"object","properties":{"lines":{"type":"integer","default":50}}},
 "annotations":{"readOnlyHint":true}},
{"name":"import","title":"Import a model",
 "description":"Import .gltf, .glb, or .j3mesh files into assets/ as reusable scenes (assets/imported/<name>.j3scene), one line back per model. path may be a folder (its models, not recursive) or contain * wildcards; paths takes a list. Only a single imported model is placed (unless place is false).",
 "inputSchema":{"type":"object","properties":{
   "path":{"type":"string","description":"Model file, folder, or wildcard like kit/wall*.glb, absolute or relative to the project."},
   "paths":{"type":"array","items":{"type":"string"}},
   "bake":{"type":"boolean","default":false,"description":"Sample each textured triangle's texture at its UV center into vertex colors and drop the texture (for palette kits like Kenney's)."},
   "place":{"type":"boolean","default":true},
   "again":{"type":"boolean","default":false,"description":"Import even when assets/imported/<name>.j3scene exists (makes <name>_2). By default that model is skipped and the existing scene is used."},
   "scene":{"type":"string"},
   "name":{"type":"string"},
   "parent":{"type":"string"},
   "pos":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}}},
{"name":"asset_search","title":"Find assets",
 "description":"Find meshes, models, materials, images, scenes, sounds, and scripts in the project, plus the built-in shapes. Paths are what asset, material, texture, scene, and script fields take.",
 "inputSchema":{"type":"object","properties":{
   "query":{"type":"string"},
   "kind":{"type":"string","enum":["any","mesh","model","material","image","scene","sound","font","script","code","file"]}}},
 "annotations":{"readOnlyHint":true}},
{"name":"asset_online_search","title":"Search online assets",
 "description":"Search free CC0 assets online: Poly Haven models and skies, ambientCG materials, and Kenney model packs (a pack is a zip of many low-poly models). One line per result: source, id, kind, name, author. Then asset_fetch with the source and id.",
 "inputSchema":{"type":"object","properties":{
   "query":{"type":"string","description":"Words to match, like chair, brick, or town. Empty lists everything (up to 40)."},
   "source":{"type":"string","enum":["any","polyhaven","ambientcg","kenney"],"default":"any"},
   "kind":{"type":"string","enum":["any","model","material","sky"],"default":"any"}}},
 "annotations":{"readOnlyHint":true}},
{"name":"asset_fetch","title":"Download an online asset",
 "description":"Download an asset asset_online_search found (cached in ~/.cache/j3/assets) and add it to the project, with a line in assets/CREDITS.txt. A Poly Haven model is imported like the import tool (and placed unless place is false). An ambientCG material becomes assets/materials/<id>.j3mat with its color texture. A Poly Haven sky becomes assets/skies/<id>.png for a builtin:sky mesh's texture. A Kenney pack without file lists its models; with file (a name from that list, with or without .glb) it imports that one. Kenney models look best with bake:true.",
 "inputSchema":{"type":"object","required":["source","id"],"properties":{
   "source":{"type":"string","enum":["polyhaven","ambientcg","kenney"]},
   "id":{"type":"string"},
   "file":{"type":"string","description":"Kenney only: the model in the pack to import."},
   "bake":{"type":"boolean","default":false},
   "place":{"type":"boolean","default":true},
   "again":{"type":"boolean","default":false},
   "scene":{"type":"string"},
   "name":{"type":"string"},
   "parent":{"type":"string"},
   "pos":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}}},
{"name":"gen","title":"Run mesh generators",
 "description":"Run the project's mesh generator scripts: each assets/gen/<name>.j3 builds a mesh in code with j3.gfx.meshgen (box, cylinder, cone, frustum, sphere, capsule, torus, lathe, extrude, heightField, tri, quad; color, glow; at, turn, size, push, pop) and ends with b.save(meshgen.output(\"assets/gen/<name>.j3mesh\")). Write the script with your file tools, then run this; use the .j3mesh as a mesh asset. Unchanged scripts are skipped unless force. Render and play run it too. `j3c doc j3.gfx.meshgen` has the API.",
 "inputSchema":{"type":"object","properties":{"force":{"type":"boolean","default":false}}}},
{"name":"scene_stats","title":"Scene draw cost",
 "description":"Run after adding many objects or tints; slow scenes are usually too many distinct looks. How much a scene costs to draw: objects (placed scenes included), meshes, distinct looks (mesh, material, texture, tint: one instanced draw call each per pass), the ten biggest looks, and cutout/blended meshes that draw one by one. Above 60 looks frame time suffers; snap tints to a small palette.",
 "inputSchema":{"type":"object","properties":{"scene":{"type":"string"}}},
 "annotations":{"readOnlyHint":true}},
{"name":"scene_diff","title":"What changed in a scene",
 "description":"Review your own work before finishing. Objects added, removed, moved (over 1 cm, with positions), and changed (field old -> new) in a scene file since git HEAD, or since another scene file (against). Matched by Parent/Child path, so renumbered ids don't matter. Reads saved files, not unsaved editor changes. Use it to review your own edits.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string"},
   "against":{"type":"string","description":"Project-relative .j3scene to compare with; default the scene at git HEAD."}}},
 "annotations":{"readOnlyHint":true}}
]"#;

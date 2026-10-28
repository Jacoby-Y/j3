//! `j3c -e mcp`: a Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0), so
//! an agent can build scenes, look at them, and play the game. It is a thin layer over the
//! same work `j3c -e scene`, `j3c -e render`, `j3c check`, and `j3c run` do.
//!
//! While `j3c editor` has the project open, scene tools act on the live editor instead of
//! the files: requests go through the editor's events file, each change is one undo step,
//! and the person watching sees it happen. Scenes the editor doesn't have open, and every
//! scene while it is closed, are edited as files.
//!
//! stdout carries only protocol messages; everything else goes to stderr.

use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::lsp::json::{self, Json};
use crate::scene::{self, Reply};

mod ops;
mod schema;
mod server;

use ops::*;
use schema::*;

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const USAGE: &str = "\
usage: j3c -e mcp [--project DIR]

Serves the Model Context Protocol on stdin/stdout for an agent working on a j3 project.
Without --project it follows the editor: the project `j3c editor` has open, else the
nearest j3.toml at or above the current directory. With --project DIR it stays on the
nearest j3.toml at or above DIR. Tools: scene_list,
scene_get, scene_add, scene_add_many, scene_set, scene_delete, place, undo, render, check, play, console, import,
asset_search, asset_online_search, asset_fetch, gen, scene_stats, scene_diff. `j3c -e mcp --tools` prints each tool's
arguments and the CLI command that does the same work.

Saving and undo depend on the editor. With `j3c editor` open on the project, scene edits go
to it live: each is one undo step there (Ctrl+Z), and nothing is saved until the person saves
or scene_set passes save: true. With it closed, each edit writes the scene file at once and
the undo tool puts the file back (20 steps). Every edit reply says which one happened.

Claude Code:  claude mcp add -s user j3 -- j3c -e mcp
.mcp.json:    {\"mcpServers\": {\"j3\": {\"command\": \"j3c\", \"args\": [\"-e\", \"mcp\"]}}}";

const INSTRUCTIONS: &str = "\
Tools for a j3 game project: scenes are trees of named objects (meshes, cameras, lights, \
groups, placed scenes) in scenes/*.j3scene. Name objects by name, by Parent/Child path, or \
by #id. Positions are local to the parent, rotations are degrees (in the tools and in scene \
files), y is up, and 1 unit is about 1 meter. A scene file is one line per object, `Name kind \
key=value #id`, indent for children; scene_get and scene_list tree=true print those lines and \
scene_apply takes them back.

Work in a loop: change the scene, LOOK at it with render, fix what looks wrong, run check.
- Build with the bulk tools, not many single calls. place: ring (round walls, slabs sized to \
close the circle), wall (with a door gap), stairs, spiral, grid (modular pieces), scatter \
(random copies, grounded), ground/ontop (rest on surfaces instead of guessing heights). \
scene_add_many batches anything else in one undo step, and scene_apply writes many varied \
objects (or a whole subtree you copied from scene_get) as scene lines in one.
- Bulk edits: scene_set and scene_delete take globs (DecoCorn*). scene_list with filter and \
group_by=prefix shows what names exist.
- Look from where the player would be: render with at [x,y,z] + yaw/pitch (degrees; yaw 0 \
faces -z, 90 faces -x), views for several angles in one image, hide (Roof*) to see inside, \
view=top with ortho for a plan. Rendering from a camera you add just to look is not needed.
- Verify before you say done: check (scene rules, the project's own rules, and code), \
scene_stats (draw cost), scene_diff (review your own changes). play runs the game headless.
- Read behaviour from tags (scene_set tags=a,b=1), not name prefixes, in new game code.
When the person has the editor open, your edits appear there as undoable steps and are not \
saved until they save (or you call scene_set with save). Without the editor, undo puts a \
scene file back as it was before your last change. Need a model, material, or sky the \
project lacks? asset_online_search, then asset_fetch.";

/// The server instructions, plus the project's own notes: the file `notes = "..."` names in
/// `j3.editor.toml` (first 4000 bytes), so a project can say what its checks and tags mean.
fn instructions(root: Option<&Path>) -> String {
    let mut text = INSTRUCTIONS.to_string();
    let Some(root) = root else { return text };
    let Some(rel) = crate::editor::config_value(root, "notes") else { return text };
    let p = Path::new(&rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) { return text; }
    if let Ok(notes) = fs::read_to_string(root.join(p)) {
        let cut = (0..=notes.len().min(4000)).rev().find(|i| notes.is_char_boundary(*i)).unwrap_or(0);
        text += &format!("\n\nThis project ({rel}):\n{}", notes[..cut].trim());
    }
    text
}

pub fn cmd(rest: &[String]) -> i32 {
    let mut project = None;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            "--tools" => {
                print!("{}", tool_reference());
                return 0;
            }
            "--project" => match it.next() {
                Some(p) => project = Some(PathBuf::from(p)),
                None => {
                    eprintln!("j3c -e mcp: --project needs a directory");
                    return 1;
                }
            },
            other => {
                eprintln!("j3c -e mcp: unknown argument `{other}`");
                return 1;
            }
        }
    }
    let follow = project.is_none();
    let root = scene::find_root(&project.unwrap_or_else(|| PathBuf::from(".")));
    let mut server = Server { root, follow, next: 0, was_live: false, coach: Coach::default() };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&mut server, stdin.lock(), &mut stdout.lock());
    0
}

struct Server {
    /// The project, or why there is none (reported by every tool call).
    root: Result<PathBuf, String>,
    /// No --project: use the project the editor has open, when one is.
    follow: bool,
    /// Counter for live editor request ids.
    next: u64,
    /// The last scene request went to the open editor, not the file.
    was_live: bool,
    coach: Coach,
}

/// What the agent has done so far, so replies can point at a better tool at the moment it
/// would have helped. Hints are short, say what to call, and each fires once per streak.
#[derive(Default)]
struct Coach {
    /// `scene_add` calls in a row.
    adds_in_a_row: u32,
    sets_in_a_row: u32,
    /// Scene edits since the last render.
    edits_unseen: u32,
    renders: u32,
    /// The scene was checked since the last edit.
    checked: bool,
}

impl Coach {
    /// Hints for a finished call to `name` with `args`.
    fn hints(&mut self, name: &str, args: &Json, failed: bool) -> Vec<String> {
        let mut out = Vec::new();
        if failed { return out; }
        if name == "scene_add" {
            self.adds_in_a_row += 1;
            if self.adds_in_a_row == 5 {
                out.push("hint: five scene_add calls in a row. For repeated shapes, place does it in one step: ring (round walls), wall (with a door), stairs, spiral, grid, scatter. scene_add_many batches other objects.".into());
            }
        } else {
            self.adds_in_a_row = 0;
        }
        if name == "scene_add_many" {
            let n = match args.get("adds") { Some(Json::Arr(a)) => a.len(), _ => 0 };
            if n >= 30 {
                out.push(format!("hint: {n} objects added by hand. If they follow a pattern, place ring/wall/stairs/spiral/grid/scatter makes it from a few numbers, and is easier to change."));
            }
        }
        if name == "scene_set" {
            self.sets_in_a_row += 1;
            if self.sets_in_a_row == 5 {
                out.push("hint: five scene_set calls in a row. If they change objects with similar names, node takes a glob (DecoCorn*) and sets all of them; scene_list with group_by=prefix shows the name groups.".into());
            }
        } else {
            self.sets_in_a_row = 0;
        }
        if EDITS.contains(&name) {
            self.edits_unseen += 1;
            self.checked = false;
            if self.edits_unseen == 8 {
                out.push("hint: eight edits since you last looked. Call render (at/yaw/pitch for inside a room, views for several angles) before building more.".into());
            }
        }
        if name == "render" {
            self.renders += 1;
            self.edits_unseen = 0;
            let from_a_spot = args.get("at").is_some() || args.get("views").is_some();
            if !from_a_spot && self.renders <= 2 {
                out.push("hint: render also takes at [x,y,z] with yaw/pitch (degrees) to look from any spot, views for up to 16 angles in one image, hide (e.g. \"Roof*\") to see into a building. Use them to check a room is reachable, not just that it exists.".into());
            }
        }
        if name == "check" {
            if !self.checked {
                out.push("hint: before you finish, scene_diff lists what you changed (review it), and scene_stats shows draw cost (above 60 distinct looks is slow).".into());
            }
            self.checked = true;
        }
        if name == "play" && !self.checked {
            out.push("hint: check hasn't run since your last edit. It is faster than play and names the object at fault.".into());
        }
        out
    }
}

fn serve(server: &mut Server, input: impl BufRead, out: &mut impl Write) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let reply = match json::parse(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(error(Json::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            let _ = writeln!(out, "{}", reply.stringify());
            let _ = out.flush();
        }
    }
}

fn result(id: Json, value: Json) -> Json {
    Json::obj(vec![("jsonrpc", Json::Str("2.0".into())), ("id", id), ("result", value)])
}

fn error(id: Json, code: i64, message: &str) -> Json {
    Json::obj(vec![
        ("jsonrpc", Json::Str("2.0".into())),
        ("id", id),
        ("error", Json::obj(vec![("code", Json::Num(code as f64)), ("message", Json::Str(message.into()))])),
    ])
}

fn s(v: &str) -> Json { Json::Str(v.into()) }

/// A tool's answer: MCP content blocks, optional structured data, and whether it failed.
struct Answer {
    content: Vec<Json>,
    structured: Option<Json>,
    error: bool,
}

impl Answer {
    fn text(text: impl Into<String>) -> Answer {
        Answer { content: vec![text_block(text)], structured: None, error: false }
    }

    fn fail(text: impl Into<String>) -> Answer {
        Answer { content: vec![text_block(text)], structured: None, error: true }
    }

    fn json(self) -> Json {
        let mut pairs = vec![("content", Json::Arr(self.content)), ("isError", Json::Bool(self.error))];
        if let Some(data) = self.structured {
            pairs.push(("structuredContent", data));
        }
        Json::obj(pairs)
    }
}

fn text_block(text: impl Into<String>) -> Json {
    Json::obj(vec![("type", s("text")), ("text", Json::Str(text.into()))])
}

#[cfg(test)]
mod tests;

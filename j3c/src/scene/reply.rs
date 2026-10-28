use super::*;

pub(super) fn node_json(l: &[String]) -> Json {
    Json::obj(vec![
        ("id", Json::Num(at(l, 1).parse().unwrap_or(0.0))),
        ("path", Json::Str(at(l, 2).into())),
        ("kind", Json::Str(at(l, 3).into())),
        ("parent", Json::Num(at(l, 4).parse().unwrap_or(0.0))),
        ("asset", Json::Str(at(l, 5).into())),
        ("worldPosition", vec3(at(l, 6))),
        ("rotation", vec3(at(l, 7))),
        ("scale", vec3(at(l, 8))),
        ("script", Json::Str(at(l, 9).into())),
        ("material", Json::Str(at(l, 10).into())),
        ("scene", Json::Str(at(l, 11).into())),
    ])
}

/// The reply as JSON: `{ok, scene, nodes|node|id|issues|message|error}`.
pub(crate) fn to_json(op: &str, scene: &str, reply: &Reply) -> Json {
    let mut pairs: Vec<(&str, Json)> = vec![("ok", Json::Bool(reply.ok)), ("scene", Json::Str(scene.into()))];
    if let Some(e) = reply.error() {
        pairs.push(("error", Json::Str(e)));
        return Json::obj(pairs);
    }
    match op {
        "list" => {
            let nodes = reply.lines.iter().filter(|l| l[0] == "node").map(|l| node_json(l)).collect();
            pairs.push(("nodes", Json::Arr(nodes)));
            let groups: Vec<(String, Json)> = reply
                .lines
                .iter()
                .filter(|l| l[0] == "group")
                .map(|l| (at(l, 1).to_string(), Json::Num(at(l, 2).parse().unwrap_or(0.0))))
                .collect();
            if !groups.is_empty() { pairs.push(("groups", Json::Obj(groups))); }
        }
        "get" => {
            let fields: Vec<(String, Json)> = reply
                .lines
                .iter()
                .filter(|l| l[0] == "field")
                .map(|l| (at(l, 1).to_string(), Json::Str(at(l, 2).into())))
                .collect();
            pairs.push(("fields", Json::Obj(fields)));
        }
        "check" => {
            let issues = reply
                .lines
                .iter()
                .filter(|l| l[0] == "issue")
                .map(|l| {
                    Json::obj(vec![
                        ("severity", Json::Str(at(l, 1).into())),
                        ("node", Json::Num(at(l, 2).parse().unwrap_or(0.0))),
                        ("message", Json::Str(l[3..].join(" "))),
                    ])
                })
                .collect();
            pairs.push(("issues", Json::Arr(issues)));
        }
        "stats" => {
            let looks = reply
                .lines
                .iter()
                .filter(|l| l[0] == "look")
                .map(|l| {
                    Json::obj(vec![
                        ("count", Json::Num(at(l, 1).parse().unwrap_or(0.0))),
                        ("asset", Json::Str(at(l, 2).into())),
                        ("material", Json::Str(at(l, 3).into())),
                        ("texture", Json::Str(at(l, 4).into())),
                        ("tint", vec3(at(l, 5))),
                        ("example", Json::Str(at(l, 6).into())),
                    ])
                })
                .collect();
            pairs.push(("looks", Json::Arr(looks)));
        }
        _ => {}
    }
    let lines: Vec<Json> = reply.lines.iter().filter(|l| l[0] == "text").map(|l| Json::Str(l[1..].join("\t"))).collect();
    if !lines.is_empty() { pairs.push(("lines", Json::Arr(lines))); }
    if let Some(l) = reply.lines.iter().find(|l| l[0] == "added") {
        pairs.push(("id", Json::Num(at(l, 1).parse().unwrap_or(0.0))));
        pairs.push(("path", Json::Str(at(l, 2).into())));
    }
    if let Some(l) = reply.lines.iter().find(|l| l[0] == "ok") {
        pairs.push(("message", Json::Str(l[1..].join(" "))));
    }
    Json::obj(pairs)
}

/// The reply for people (and agents reading plain text).
pub(crate) fn to_text(reply: &Reply) -> String {
    let mut out = String::new();
    for l in &reply.lines {
        match l[0].as_str() {
            "node" => {
                let mut row = format!("#{:<5} {:<32} {:<8}", at(l, 1), at(l, 2), at(l, 3));
                if !at(l, 5).is_empty() { row += &format!(" {}", at(l, 5)); }
                if !at(l, 11).is_empty() { row += &format!(" {}", at(l, 11)); }
                row += &format!("  at {}", at(l, 6));
                if !at(l, 9).is_empty() { row += &format!("  [{}]", at(l, 9)); }
                out += row.trim_end();
                out.push('\n');
            }
            "field" => out += &format!("{}: {}\n", at(l, 1), at(l, 2)),
            "group" => out += &format!("{:>6}  {}*\n", at(l, 2), at(l, 1)),
            "added" => out += &format!("added #{} {}\n", at(l, 1), at(l, 2)),
            "issue" => out += &format!("{}: {}\n", at(l, 1), l[3..].join(" ")),
            "look" => out += &format!("  {:>5}x {} {} {} tint {}  (e.g. {})\n", at(l, 1), at(l, 2), at(l, 3), at(l, 4), at(l, 5), at(l, 6)),
            "text" => out += &format!("{}\n", l[1..].join("\t")),
            "ok" => out += &format!("{}\n", l[1..].join(" ")),
            "err" => out += &format!("error: {}\n", l[1..].join(" ")),
            _ => out += &format!("{}\n", l.join(" ")),
        }
    }
    out
}

pub(super) fn vec3(s: &str) -> Json {
    Json::Arr(s.split(',').map(|v| Json::Num(v.parse().unwrap_or(0.0))).collect())
}

pub(super) fn at(line: &[String], i: usize) -> &str {
    line.get(i).map(String::as_str).unwrap_or("")
}

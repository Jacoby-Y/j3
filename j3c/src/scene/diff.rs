use super::*;

/// What `diff` found, one line per object.
pub(crate) struct Diff {
    pub against: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub moved: Vec<String>,
    pub changed: Vec<String>,
}

impl Diff {
    /// Counts first, then up to `cap` lines.
    pub(crate) fn text(&self, cap: usize) -> String {
        let mut out = format!(
            "{} added, {} removed, {} moved, {} changed since {}\n",
            self.added.len(),
            self.removed.len(),
            self.moved.len(),
            self.changed.len(),
            self.against
        );
        let all: Vec<String> = self.added.iter().map(|l| format!("+ {l}"))
            .chain(self.removed.iter().map(|l| format!("- {l}")))
            .chain(self.moved.iter().map(|l| format!("~ {l}")))
            .chain(self.changed.iter().map(|l| format!("* {l}")))
            .collect();
        for l in all.iter().take(cap) { out += &format!("{l}\n"); }
        if all.len() > cap { out += &format!("... and {} more\n", all.len() - cap); }
        out
    }
}

/// A scene file's objects with their `Parent/Child` path; same-path objects get `#2`, `#3`, ...
/// in file order. Reads either file version.
pub(super) fn scene_objects(text: &str) -> Vec<(String, crate::scenefile::Object)> {
    use std::collections::HashMap;
    let (objects, _) = crate::scenefile::parse(text);
    let by_id: HashMap<i64, &crate::scenefile::Object> = objects.iter().map(|o| (o.id, o)).collect();
    let path_of = |o: &crate::scenefile::Object| {
        let mut parts = vec![o.name.clone()];
        let mut parent = o.parent;
        while parent != 0 && parts.len() < 64 {
            let Some(p) = by_id.get(&parent) else { break };
            parts.push(p.name.clone());
            parent = p.parent;
        }
        parts.reverse();
        parts.join("/")
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for o in &objects {
        let mut path = path_of(o);
        let n = seen.entry(path.clone()).or_insert(0);
        *n += 1;
        if *n > 1 { path = format!("{path}#{n}"); }
        out.push((path, o.clone()));
    }
    out
}

/// `diff`: `rel` now against `old` (project-relative), or against git HEAD.
// ponytail: positions are the parent's space (world for top-level objects), not world.
pub(crate) fn diff(root: &Path, rel: &str, old: Option<&str>) -> Result<Diff, String> {
    let now = fs::read_to_string(root.join(rel)).map_err(|e| format!("read {rel}: {e}"))?;
    let (before, against) = match old {
        Some(o) => {
            let o = scene_rel(root, Some(o))?;
            (fs::read_to_string(root.join(&o)).map_err(|e| format!("read {o}: {e}"))?, o)
        }
        None => {
            let out = Command::new("git")
                .current_dir(root)
                .args(["show", &format!("HEAD:./{rel}")])
                .output()
                .map_err(|e| format!("run git: {e}"))?;
            // Not in HEAD yet: everything is new.
            let text = if out.status.success() { String::from_utf8_lossy(&out.stdout).into_owned() } else { String::new() };
            (text, "git HEAD".to_string())
        }
    };
    let old = scene_objects(&before);
    let new = scene_objects(&now);
    let old_map: std::collections::HashMap<&str, &crate::scenefile::Object> =
        old.iter().map(|(p, o)| (p.as_str(), o)).collect();
    let new_paths: std::collections::HashSet<&str> = new.iter().map(|(p, _)| p.as_str()).collect();
    let show = |p: [f64; 3]| format!("{:.2},{:.2},{:.2}", p[0], p[1], p[2]);
    let mut d = Diff { against, added: Vec::new(), removed: Vec::new(), moved: Vec::new(), changed: Vec::new() };
    for (path, o) in &new {
        let Some(was) = old_map.get(path.as_str()) else {
            d.added.push(format!("{path} at {}", show(o.pos)));
            continue;
        };
        let (a, b) = (was.pos, o.pos);
        let dist = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        if dist > 0.01 { d.moved.push(format!("{path} {} -> {} ({dist:.2} m)", show(a), show(b))); }
        let before: std::collections::BTreeMap<String, String> = was.fields().into_iter().collect();
        let now: std::collections::BTreeMap<String, String> = o.fields().into_iter().collect();
        let mut fields = Vec::new();
        for key in before.keys().chain(now.keys()).collect::<std::collections::BTreeSet<_>>() {
            let (x, y) = (before.get(key), now.get(key));
            if x != y {
                fields.push(format!("{key} {} -> {}", x.map(String::as_str).unwrap_or("(default)"), y.map(String::as_str).unwrap_or("(default)")));
            }
        }
        if !fields.is_empty() { d.changed.push(format!("{path}: {}", fields.join(", "))); }
    }
    for (path, f) in &old {
        if !new_paths.contains(path.as_str()) { d.removed.push(format!("{path} at {}", show(f.pos))); }
    }
    Ok(d)
}

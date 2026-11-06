//! Import cycle check over the parsed files.

use super::*;

pub(in crate::compile) fn check_import_cycles(files: &[ParsedFile]) -> Result<(), String> {
    let keys: Vec<String> = files.iter().map(|f| f.ast.module_key()).collect();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for f in files {
        let from = f.ast.module_key();
        for it in &f.ast.items {
            if let parse::Item::Use(u) = it {
                if let Some(to) = use_mod_key(u, &keys) {
                    if to != from {
                        graph.entry(from.clone()).or_default().push(to);
                    }
                }
            }
        }
        for script in script_deps::find(&f.ast) {
            let to = script_module_parts(&script.path)?.join(".");
            if to != from {
                graph.entry(from.clone()).or_default().push(to);
            }
        }
    }
    let mut done = HashSet::new();
    for start in &keys {
        if let Some(cycle) = cycle_from(&graph, start, &mut done) {
            return Err(format!("import cycle: {}", cycle.join(" -> ")));
        }
    }
    Ok(())
}

pub(super) fn use_mod_key(u: &parse::UseItem, loaded: &[String]) -> Option<String> {
    let parts: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
    if u.names.is_some() {
        let k = parts.join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
        return None;
    }
    let as_mod = parts.join(".");
    if loaded.iter().any(|k| k == &as_mod) {
        return Some(as_mod);
    }
    if parts.len() >= 2 {
        let k = parts[..parts.len() - 1].join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
    }
    None
}

pub(super) fn cycle_from(
    graph: &HashMap<String, Vec<String>>,
    start: &str,
    done: &mut HashSet<String>,
) -> Option<Vec<String>> {
    fn dfs(
        graph: &HashMap<String, Vec<String>>,
        node: &str,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if on_stack.contains(node) {
            let i = stack.iter().position(|s| s == node)?;
            let mut c = stack[i..].to_vec();
            c.push(node.to_string());
            return Some(c);
        }
        if done.contains(node) {
            return None;
        }
        stack.push(node.to_string());
        on_stack.insert(node.to_string());
        if let Some(nexts) = graph.get(node) {
            for n in nexts {
                if let Some(c) = dfs(graph, n, stack, on_stack, done) {
                    return Some(c);
                }
            }
        }
        on_stack.remove(node);
        stack.pop();
        done.insert(node.to_string());
        None
    }
    dfs(graph, start, &mut Vec::new(), &mut HashSet::new(), done)
}

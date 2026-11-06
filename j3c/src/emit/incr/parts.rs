//! Split modules: the `.parts` file, per-part object paths, and the giant-function split plan.

use super::*;

pub(super) fn parts_file(obj: &Path) -> PathBuf {
    obj.with_extension("parts")
}

/// Object `n` of a split module.
pub(super) fn part_path(obj: &Path, n: usize) -> PathBuf {
    obj.with_extension(format!("p{n}.o"))
}

/// How many objects a module was split into last time (0 = one object).
pub(super) fn parts_of(obj: &Path) -> usize {
    fs::read_to_string(parts_file(obj)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// Byte length of each `define` block in a module's LLVM text, by symbol.
pub(super) fn define_sizes(ll: &str) -> HashMap<&str, usize> {
    let mut sizes = HashMap::new();
    let mut open: Option<(&str, usize)> = None;
    for line in ll.lines() {
        if let Some(rest) = line.strip_prefix("define ") {
            let name = rest.split('@').nth(1).and_then(|r| r.split('(').next()).unwrap_or("");
            open = Some((name, 0));
        }
        if let Some((name, n)) = open.as_mut() {
            *n += line.len() + 1;
            if line == "}" {
                sizes.insert(*name, *n);
                open = None;
            }
        }
    }
    sizes
}

/// A function this large makes clang's backend slow on its own (it grows faster than linearly).
const GIANT_FN: usize = 15_000;

/// Split a module with several giant functions: one set of functions per object, balanced by
/// the size of their LLVM text. None when it is not worth it (or `[emit] chunks = 0`).
pub(super) fn split_plan(cx: &IrCx, module: &Module, ll: &str, opts: &EmitOpts) -> Option<Vec<HashSet<FuncId>>> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let cap = opts.chunks.unwrap_or(4).min((threads / 2).max(1));
    if cap < 2 {
        return None;
    }
    let sizes = define_sizes(ll);
    let funcs: Vec<(FuncId, usize)> = module
        .funcs
        .iter()
        .filter_map(|id| {
            let func = cx.func(*id);
            let size = *sizes.get(c_expr::func_c_name(cx, func).as_str())?;
            Some((*id, size))
        })
        .collect();
    let giants = funcs.iter().filter(|f| f.1 >= GIANT_FN).count();
    let (total, max) = (funcs.iter().map(|f| f.1).sum::<usize>(), funcs.iter().map(|f| f.1).max().unwrap_or(0));
    let mut n = giants.min(cap);
    if n < 2 && max >= GIANT_FN && total - max >= max / 3 {
        n = 2; // one giant: at least take everything else off its object
    }
    if n < 2 {
        return None;
    }
    let weights: Vec<usize> = funcs.iter().map(|f| f.1).collect();
    let mut sets = vec![HashSet::new(); n];
    for ((id, _), c) in funcs.iter().zip(llvm::chunk_assign(&weights, n)) {
        sets[c].insert(*id);
    }
    Some(sets)
}

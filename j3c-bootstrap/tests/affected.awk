# Which unit tests a change can reach. Input: `grep -H` lines of `module X;` and `use X...;` from the compiler,
# stdlib and unit test sources, then (after a `--CHANGED--` line) the changed repo files, one per line.
# A use path names its longest known module prefix (`use j3c.sema.query.{a}` -> j3c.sema.query, or j3c.sema).
# Prints the test names (tests/unit/NAME_test.j3) whose sources use a changed module, directly or through
# modules that use it, plus tests whose own file or expected files changed.
# ponytail: a file's module is the module it declares; a part file counts for its whole module.
/^--CHANGED--$/ { ch = 1; next }
!ch {
    f = $0; sub(/:.*/, "", f); l = substr($0, length(f) + 2)
    if (l ~ /^module / && f ~ /_test\.j3$/) next   # a test is named by its file, not its module
    if (l ~ /^module /) { m = l; sub(/^module /, "", m); sub(/;.*/, "", m); modof[f] = m; known[m] = 1; next }
    p = l; sub(/^ *(pub )?use /, "", p); sub(/[;{ ].*/, "", p); sub(/\.$/, "", p)
    uses[++n] = f; path[n] = p; next
}
{ changed[++c] = $0 }
END {
    for (i = 1; i <= n; i++) {
        f = uses[i]; m = (f in modof) ? modof[f] : "test:" f
        p = path[i]
        while (p != "" && !(p in known)) { if (p !~ /\./) { p = ""; break } sub(/\.[^.]*$/, "", p) }
        if (p != "" && p != m) users[p] = users[p] " " m
    }
    for (k = 1; k <= c; k++) {
        f = changed[k]
        if (f ~ /^j3c-bootstrap\/tests\/unit\/[^\/]*_test\.j3$/) { t = f; sub(/.*\//, "", t); sub(/_test\.j3$/, "", t); print t; continue }
        if (f ~ /^j3c-bootstrap\/tests\/unit\/expect\//) { t = f; sub(/.*\//, "", t); sub(/\..*/, "", t); print "expect:" t; continue }
        if (f ~ /^j3c-bootstrap\/tests\/unit\/kit\// || f == "j3c-bootstrap/j3.toml") { print "ALL"; continue }
        r = f; if (sub(/^j3c-bootstrap\//, "", r) == 0) r = "../" f
        if (r in modof) hit[modof[r]] = 1
    }
    # close over users
    do { more = 0
        for (m in hit) { k = split(users[m], u, " "); for (j = 1; j <= k; j++) if (!(u[j] in hit)) { hit[u[j]] = 1; more = 1 } }
    } while (more)
    for (m in hit) if (m ~ /^test:/) { t = m; sub(/^test:.*\//, "", t); sub(/_test\.j3$/, "", t); print t }
}

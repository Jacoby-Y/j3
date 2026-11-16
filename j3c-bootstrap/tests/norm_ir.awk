# `diff.sh ir --norm` (plan 11 A6): renumber compiler-made locals and temps (`_t3`, `_ctxp2`, `$each18`)
# by order of first appearance in each function, so two lowerings that differ only in numbering compare
# equal. A token counts when nothing identifier-like touches it (`main$closure0`, `x._t1` stay as they are).
# Numbering restarts at every unindented line (each `fn`, `struct`, ...). Output: `_t3` -> `_t#0`.
# ponytail: regex on the dump text; user-named locals are left alone (they're stable).
/^[^ \t]/ { delete m; n = 0 }
{
    line = $0; out = ""; pos = 1
    while (match(substr(line, pos), /[_$][a-z]+[0-9]+/)) {
        s = pos + RSTART - 1; e = s + RLENGTH
        tok = substr(line, s, RLENGTH)
        before = s > 1 ? substr(line, s - 1, 1) : ""
        out = out substr(line, pos, s - pos)
        if (before ~ /[A-Za-z0-9_$.]/ || substr(line, e, 1) ~ /[A-Za-z0-9_]/) {
            out = out tok
        } else {
            if (!(tok in m)) m[tok] = n++
            p = tok; sub(/[0-9]+$/, "", p)
            out = out p "#" m[tok]
        }
        pos = e
    }
    print out substr(line, pos)
}

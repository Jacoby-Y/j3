//! TextMate repository rules for tokens: keywords, types, numbers, calls, members, operators.

use super::*;

pub(super) fn repo_keywords(out: &mut String) {
    let groups = grouped_keywords();
    out.push_str("    \"keywords\": {\n");
    out.push_str("      \"patterns\": [\n");
    // Contextual: `trait Name`, so a variable called `trait` stays a variable.
    match_rule(out, 4, "storage.type.trait.j3", &format!("\\b{}(?=\\s+{})", ITEM_TRAIT, IDENT), true);
    match_rule(out, 4, "storage.type.extend.j3", &format!("\\b{}(?=\\s+{})", ITEM_EXTEND, IDENT), true);
    for (i, (scope, names)) in groups.iter().enumerate() {
        let last = i + 1 == groups.len();
        let refs: Vec<&str> = names.iter().copied().collect();
        match_rule(out, 4, scope, &word_alts(&refs), !last);
    }
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn grouped_keywords() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut groups: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for &(name, kind) in KEYWORDS {
        let scope = kind
            .highlight_scope()
            .expect("keyword needs a highlight scope");
        if let Some((_, names)) = groups.iter_mut().find(|(s, _)| *s == scope) {
            names.push(name);
        } else {
            groups.push((scope, vec![name]));
        }
    }
    groups
}

pub(super) fn repo_types(out: &mut String) {
    let slice = format!("(\\[)(\\])(?={})", IDENT);
    let array = format!("(\\[)([0-9]+)(\\])(?={})", IDENT);
    let generic_begin = format!("\\b({})\\s*(<)", PASCAL);
    out.push_str("    \"types\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &slice,
        &[
            ("1", "punctuation.definition.bracket.square.j3"),
            ("2", "punctuation.definition.bracket.square.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &array,
        &[
            ("1", "punctuation.definition.bracket.square.j3"),
            ("2", "constant.numeric.integer.j3"),
            ("3", "punctuation.definition.bracket.square.j3"),
        ],
        true,
    );
    match_rule(
        out,
        4,
        "support.type.builtin.j3",
        &word_alts(BUILTIN_TYPES),
        true,
    );
    match_rule(
        out,
        4,
        "support.type.builtin.j3",
        &format!("\\b{}(?=\\()", TYPE_CLOSURE),
        true,
    );
    out.push_str("        {\n");
    out.push_str("          \"begin\": ");
    json_string(out, &generic_begin);
    out.push_str(",\n");
    out.push_str("          \"beginCaptures\": {\n");
    out.push_str("            \"1\": { \"name\": \"entity.name.type.j3\" },\n");
    out.push_str(
        "            \"2\": { \"name\": \"punctuation.definition.typeparameters.begin.j3\" }\n",
    );
    out.push_str("          },\n");
    out.push_str("          \"end\": \">\",\n");
    out.push_str("          \"endCaptures\": {\n");
    out.push_str(
        "            \"0\": { \"name\": \"punctuation.definition.typeparameters.end.j3\" }\n",
    );
    out.push_str("          },\n");
    out.push_str("          \"patterns\": [\n");
    include_items(out, 6, &["comments", "types", "punctuation"]);
    out.push_str("          ]\n");
    out.push_str("        },\n");
    match_rule(
        out,
        4,
        "entity.name.type.j3",
        &format!("\\b{}\\b", PASCAL),
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_constants_bool(out: &mut String) {
    out.push_str("    \"constants-bool\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "constant.language.boolean.j3",
        &word_alts(BOOL_LITS),
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_numbers(out: &mut String) {
    out.push_str("    \"numbers\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "constant.numeric.hex.j3",
        "\\b0x[0-9A-Fa-f]+\\b",
        true,
    );
    match_rule(out, 4, "constant.numeric.binary.j3", "\\b0b[01]+\\b", true);
    match_rule(
        out,
        4,
        "constant.numeric.float.j3",
        "\\b[0-9]+\\.[0-9]+\\b",
        true,
    );
    match_rule(out, 4, "constant.numeric.integer.j3", "\\b[0-9]+\\b", false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

/// Loop labels (`'rows:`, `break 'rows;`). The lexer emits them as `TokenKind::Label`.
pub(super) fn repo_labels(out: &mut String) {
    out.push_str("    \"labels\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(out, 4, "entity.name.label.j3", "'[A-Za-z_][A-Za-z0-9_]*", false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

/// Order word as the last argument of `@atomic*(..)`. Contextual (after a comma, before `)`), so `release` stays a name elsewhere.
pub(super) fn repo_atomic_orders(out: &mut String) {
    let pat = format!("(?<=,)[ \\t]*{}(?=\\s*\\))", word_alts(ATOMIC_ORDERS));
    out.push_str("    \"atomic-orders\": {\n");
    out.push_str("      \"match\": ");
    json_string(out, &pat);
    out.push_str(",\n");
    out.push_str("      \"name\": \"constant.language.atomic-order.j3\"\n");
    out.push_str("    }");
}

pub(super) fn repo_calls(out: &mut String) {
    let turbofish = format!("\\b({})(?=\\s*::<)", IDENT);
    let call = format!("\\b({})(?=\\s*\\()", IDENT);
    out.push_str("    \"calls\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &turbofish,
        &[("1", "entity.name.function.j3")],
        true,
    );
    match_captures(out, 4, &call, &[("1", "entity.name.function.j3")], false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_members(out: &mut String) {
    let method = format!("(\\.)({})(?=\\s*\\()", CAMEL);
    let enum_call = format!("(\\.)({}|{})(?=\\s*\\()", PASCAL, CONST_NAME);
    let const_mem = format!("(\\.)({})\\b", CONST_NAME);
    let enum_mem = format!("(\\.)({})\\b", PASCAL);
    let prop = format!("(\\.)({})\\b", IDENT);
    out.push_str("    \"members\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &method,
        &[
            ("1", "punctuation.accessor.j3"),
            ("2", "entity.name.function.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &enum_call,
        &[
            ("1", "punctuation.accessor.j3"),
            ("2", "variable.other.enummember.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &const_mem,
        &[
            ("1", "punctuation.accessor.j3"),
            ("2", "variable.other.constant.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &enum_mem,
        &[
            ("1", "punctuation.accessor.j3"),
            ("2", "variable.other.enummember.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &prop,
        &[
            ("1", "punctuation.accessor.j3"),
            ("2", "variable.other.property.j3"),
        ],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_constants_named(out: &mut String) {
    out.push_str("    \"constants-named\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "variable.other.constant.j3",
        &format!("\\b{}\\b", CONST_NAME),
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_variables(out: &mut String) {
    let binding = format!("({})\\s*(:)", IDENT);
    out.push_str("    \"variables\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &binding,
        &[
            ("1", "variable.parameter.j3"),
            ("2", "punctuation.separator.j3"),
        ],
        true,
    );
    match_rule(
        out,
        4,
        "variable.other.j3",
        &format!("\\b{}\\b", IDENT),
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_operators(out: &mut String) {
    let mut groups: Vec<(&'static str, Vec<&str>)> = Vec::new();
    for k in OPERATOR_KINDS {
        let src = k.operator_src().expect("operator kind");
        let scope = k.operator_class().expect("operator class");
        if let Some((_, ops)) = groups.iter_mut().find(|(s, _)| *s == scope) {
            ops.push(src);
        } else {
            groups.push((scope, vec![src]));
        }
    }
    out.push_str("    \"operators\": {\n");
    out.push_str("      \"patterns\": [\n");
    for (i, (scope, ops)) in groups.iter().enumerate() {
        let mut ops = ops.clone();
        ops.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        let mut pat = String::new();
        for (j, op) in ops.iter().enumerate() {
            if j > 0 {
                pat.push('|');
            }
            pat.push_str(&re_escape(op));
        }
        match_rule(out, 4, scope, &pat, i + 1 != groups.len());
    }
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_punctuation(out: &mut String) {
    out.push_str("    \"punctuation\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "punctuation.bracket.j3",
        "[\\(\\)\\[\\]\\{\\}]",
        true,
    );
    match_rule(out, 4, "punctuation.separator.j3", "[,;]", true);
    match_rule(out, 4, "punctuation.separator.j3", ":", false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

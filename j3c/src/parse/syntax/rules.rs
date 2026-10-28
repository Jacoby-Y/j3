use super::*;

pub(super) fn repo_comments(out: &mut String) {
    out.push_str("    \"comments\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(out, 4, "comment.line.double-slash.j3", "//.*$", false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_strings(out: &mut String) {
    out.push_str("    \"strings\": {\n");
    out.push_str("      \"name\": \"string.quoted.double.j3\",\n");
    out.push_str("      \"begin\": \"\\\"\",\n");
    out.push_str("      \"end\": \"\\\"\",\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "constant.character.escape.j3",
        "\\\\[\\\\\"nrt0]",
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_interpolate(out: &mut String) {
    out.push_str("    \"interpolate\": {\n");
    out.push_str("      \"name\": \"string.interpolated.j3\",\n");
    out.push_str("      \"begin\": \"`\",\n");
    out.push_str("      \"end\": \"`\",\n");
    out.push_str("      \"patterns\": [\n");
    out.push_str("        {\n");
    out.push_str("          \"name\": \"meta.template.expression.j3\",\n");
    out.push_str("          \"begin\": \"\\\\{\",\n");
    out.push_str("          \"beginCaptures\": {\n");
    out.push_str("            \"0\": { \"name\": \"punctuation.definition.template-expression.begin.j3\" }\n");
    out.push_str("          },\n");
    out.push_str("          \"end\": \"\\\\}\",\n");
    out.push_str("          \"endCaptures\": {\n");
    out.push_str(
        "            \"0\": { \"name\": \"punctuation.definition.template-expression.end.j3\" }\n",
    );
    out.push_str("          },\n");
    out.push_str("          \"patterns\": [\n");
    include_items(
        out,
        6,
        &[
            "comments",
            "keywords",
            "types",
            "constants-bool",
            "numbers",
            "calls",
            "members",
            "constants-named",
            "variables",
            "operators",
        ],
    );
    out.push_str("          ]\n");
    out.push_str("        }\n");
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_attributes(out: &mut String) {
    out.push_str("    \"attributes\": {\n");
    out.push_str("      \"name\": \"meta.attribute.j3\",\n");
    out.push_str("      \"begin\": \"(#)(\\\\[)\",\n");
    out.push_str("      \"beginCaptures\": {\n");
    out.push_str("        \"1\": { \"name\": \"punctuation.definition.annotation.j3\" },\n");
    out.push_str("        \"2\": { \"name\": \"punctuation.definition.bracket.square.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"end\": \"\\\\]\",\n");
    out.push_str("      \"endCaptures\": {\n");
    out.push_str("        \"0\": { \"name\": \"punctuation.definition.bracket.square.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"patterns\": [\n");
    match_rule(
        out,
        4,
        "entity.other.attribute.j3",
        &word_alts(ATTRIBUTES),
        true,
    );
    include_items(out, 4, &["comments", "types", "punctuation"]);
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_intrinsics(out: &mut String) {
    let mut pat = String::from("(@)(");
    for (i, n) in INTRINSICS.iter().enumerate() {
        if i > 0 {
            pat.push('|');
        }
        pat.push_str(&re_escape(n));
    }
    pat.push_str(")\\b");
    out.push_str("    \"intrinsics\": {\n");
    out.push_str("      \"match\": ");
    json_string(out, &pat);
    out.push_str(",\n");
    out.push_str("      \"captures\": {\n");
    out.push_str("        \"1\": { \"name\": \"punctuation.definition.keyword.j3\" },\n");
    out.push_str("        \"2\": { \"name\": \"support.function.intrinsic.j3\" }\n");
    out.push_str("      }\n");
    out.push_str("    }");
}

pub(super) fn repo_module_decl(out: &mut String) {
    let pat = format!(
        "\\b({})\\s+({}(?:\\.{})*)",
        re_escape(kw(TokenKind::Module)),
        IDENT,
        IDENT
    );
    out.push_str("    \"module-decl\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &pat,
        &[("1", "keyword.other.j3"), ("2", "entity.name.namespace.j3")],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_use_lib(out: &mut String) {
    let begin = format!(
        "\\b({})\\s+({})\\b",
        re_escape(kw(TokenKind::Use)),
        re_escape(kw(TokenKind::Lib))
    );
    out.push_str("    \"use-lib\": {\n");
    out.push_str("      \"begin\": ");
    json_string(out, &begin);
    out.push_str(",\n");
    out.push_str("      \"beginCaptures\": {\n");
    out.push_str("        \"1\": { \"name\": \"keyword.other.j3\" },\n");
    out.push_str("        \"2\": { \"name\": \"keyword.other.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"end\": \"\\\\}\",\n");
    out.push_str("      \"endCaptures\": {\n");
    out.push_str("        \"0\": { \"name\": \"punctuation.section.block.end.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"patterns\": [\n");
    include_items(
        out,
        4,
        &[
            "comments",
            "strings",
            "fn-decl",
            "keywords",
            "types",
            "variables",
            "operators",
            "punctuation",
        ],
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_use_decl(out: &mut String) {
    let begin = format!(
        "\\b({})\\b(?!\\s+{})",
        re_escape(kw(TokenKind::Use)),
        re_escape(kw(TokenKind::Lib))
    );
    out.push_str("    \"use-decl\": {\n");
    out.push_str("      \"begin\": ");
    json_string(out, &begin);
    out.push_str(",\n");
    out.push_str("      \"beginCaptures\": {\n");
    out.push_str("        \"0\": { \"name\": \"keyword.other.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"end\": \";\",\n");
    out.push_str("      \"endCaptures\": {\n");
    out.push_str("        \"0\": { \"name\": \"punctuation.terminator.statement.j3\" }\n");
    out.push_str("      },\n");
    out.push_str("      \"patterns\": [\n");
    include_items(
        out,
        4,
        &[
            "comments",
            "keywords",
            "types",
            "constants-named",
            "calls",
            "members",
            "variables",
            "punctuation",
        ],
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_fn_decl(out: &mut String) {
    let fn_kw = re_escape(kw(TokenKind::Fn));
    let op_kw = re_escape(kw(TokenKind::Operator));
    let op_fn = format!("\\b({})\\s+({})({})", fn_kw, op_kw, op_alts());
    let named = format!("\\b({})\\s+({})", fn_kw, IDENT);
    out.push_str("    \"fn-decl\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &op_fn,
        &[
            ("1", "storage.type.j3"),
            ("2", "keyword.other.j3"),
            ("3", "entity.name.operator.j3"),
        ],
        true,
    );
    match_captures(
        out,
        4,
        &named,
        &[("1", "storage.type.j3"), ("2", "entity.name.function.j3")],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_type_decl(out: &mut String) {
    let pat = format!(
        "\\b({}|{})\\s+({})",
        re_escape(kw(TokenKind::Struct)),
        re_escape(kw(TokenKind::Enum)),
        IDENT
    );
    out.push_str("    \"type-decl\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &pat,
        &[("1", "storage.type.j3"), ("2", "entity.name.type.j3")],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_const_decl(out: &mut String) {
    let pat = format!("\\b({})\\s+({})", re_escape(kw(TokenKind::Const)), IDENT);
    out.push_str("    \"const-decl\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &pat,
        &[
            ("1", "storage.type.j3"),
            ("2", "variable.other.constant.j3"),
        ],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_let_decl(out: &mut String) {
    let pat = format!("\\b({})\\s+({})", re_escape(kw(TokenKind::Let)), IDENT);
    out.push_str("    \"let-decl\": {\n");
    out.push_str("      \"patterns\": [\n");
    match_captures(
        out,
        4,
        &pat,
        &[("1", "storage.type.j3"), ("2", "variable.other.j3")],
        false,
    );
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_script_block(out: &mut String) {
    out.push_str("    \"script-block\": {\n");
    out.push_str("      \"patterns\": [\n");
    let pat = format!("\\b({}|{}|events)\\b(?=\\s*\\{{)", SCRIPT_PROPS, SCRIPT_SHARED);
    match_captures(out, 4, &pat, &[("1", "keyword.other.j3")], true);
    // `.j3ui` script statements: `provide x;` `inject x: T;` `emit pick(i)`
    let pat = "\\b(provide|inject|emit)\\b(?=\\s+[A-Za-z_])";
    match_captures(out, 4, pat, &[("1", "keyword.other.j3")], false);
    out.push_str("      ]\n");
    out.push_str("    }");
}

pub(super) fn repo_keywords(out: &mut String) {
    let groups = grouped_keywords();
    out.push_str("    \"keywords\": {\n");
    out.push_str("      \"patterns\": [\n");
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

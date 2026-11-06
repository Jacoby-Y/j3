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

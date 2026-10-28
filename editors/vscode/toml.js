// Completion, hover, and checks for j3.toml. Keys match j3c's manifest parser.
const vscode = require("vscode");

const OPTS = ["0", "1", "2", "3", "s", "z"];
const BOOLS = ["true", "false"];

const SCHEMA = {
  package: {
    doc: "Package identity. `name` is required. `entry` defaults to `src/main.j3`.",
    keys: {
      name: { doc: "Binary name. `build` writes `target/<name>`." },
      entry: {
        doc: "File that contains `main`. Default `src/main.j3`.",
        suggest: ['"src/main.j3"'],
      },
      version: { doc: "Package version (the `.deb` version). Default `0.1.0`.", suggest: ['"0.1.0"'] },
      description: { doc: "One line for the `.deb` and the menu entry." },
    },
  },
  cc: {
    doc: "C compiler options. `[emit] opt` inherits `[cc] opt` when `[emit]` is omitted.",
    keys: {
      std: { doc: "C standard. Default `c11`.", suggest: ["c11", "c17", "c99"] },
      opt: {
        doc: "Optimization. `0` compiles faster, `2` runs faster.",
        values: OPTS,
      },
      fast: { doc: "Strip bounds-check aborts. Default `false`.", values: BOOLS },
    },
  },
  emit: {
    doc: "Backend, optimization, debug, and target triples. LLVM is the default. `target` is one triple or an array; `run` uses the host triple and `build` emits each one.",
    keys: {
      backend: { doc: "`llvm` (default) or `c`.", values: ["llvm", "c"] },
      opt: {
        doc: "`0`, `1`, `2`, `3`, `s`, or `z`. LLVM default is `2`.",
        values: OPTS,
      },
      incremental: {
        doc: 'At opt 0, reuse one object per module. Default `true`. `"all"` does it at every opt (faster rebuilds, no cross-module inlining); `--release` ignores it.',
        values: ["true", "false", "all"],
      },
      debug: { doc: "Debug info. Default `true`, unless `[cc] fast` is set.", values: BOOLS },
      fast: { doc: "Strip bounds-check aborts, same as `--fast`.", values: BOOLS },
      target: {
        doc: "One triple, or an array of triples. Empty means the host. With several, `build` writes `target/<triple>/<name>`.",
        suggest: [
          "x86_64-unknown-linux-gnu",
          "x86_64-pc-windows-gnu",
          "aarch64-apple-darwin",
          "x86_64-apple-darwin",
        ],
        list: true,
      },
      cpu: { doc: "CPU passed to clang, such as `native`." },
      clang: { doc: "Clang executable. Default `clang`.", suggest: ["clang"] },
      flags: { doc: "Extra flags for clang. An array of strings.", list: true },
    },
  },
  link: {
    doc: "Libraries and flags passed to the linker. `libs` are names without `-l`.",
    keys: {
      libs: {
        doc: "Library names (`SDL2` links as `-lSDL2`).",
        list: true,
        suggest: ["SDL2", "SDL2_image", "SDL2_mixer", "SDL2_ttf", "GL", "m"],
      },
      search: { doc: "Library search directories (`-L`).", list: true },
      files: { doc: "Extra object files or archives.", list: true },
      flags: { doc: "Extra linker flags, such as an rpath.", list: true },
    },
  },
  dist: {
    doc: "Packaging. `build_outputs = [\"zip\", \"deb\"]` makes `build --release` write them per target to `target/dist/`.",
    keys: {
      build_outputs: {
        doc: "What `build --release` writes to `target/dist/`: `zip` (program, `include`, Windows DLLs), `deb` (Linux: installs to `/opt/<name>` with a launcher, menu entry, and icon), and `apk` (Android, needs an Android triple in `[emit] target`; `j3c doc android`).",
        list: true,
        suggest: ['"zip"', '"deb"', '"apk"'],
      },
      icon: { doc: "`.png` (square 256 or 512) or `.svg` for the `.deb` menu entry.", suggest: ['"assets/icon.png"'] },
      title: { doc: "Menu name. Default: the package name, title-cased." },
      categories: { doc: "`.desktop` Categories. Default `Game;`.", suggest: ['"Game;"'] },
      maintainer: { doc: "`Name <email>`. Default `$DEBFULLNAME`/`$DEBEMAIL`, else git config." },
      depends: { doc: "Extra `.deb` Depends, after the ones found from the libraries the program loads.", list: true },
      zip: {
        doc: "`true` is `build_outputs = [\"zip\"]`: `build --release` also writes `target/dist/<name>-<os>-<arch>.zip` with a `<name>/` folder holding the program, the `include` list, and (Windows) the DLLs the .exe imports, found via `[link] search`.",
        values: BOOLS,
      },
      include: {
        doc: "Files and folders to ship. Default: whichever of `assets`, `scripts`, `scenes` exist.",
        list: true,
        suggest: ['"assets"', '"scripts"', '"scenes"'],
      },
    },
  },
  android: {
    doc: "Android APK (`build_outputs = [\"apk\"]`, `j3c doc android`).",
    keys: {
      package: { doc: "Java application id, 2+ dot-separated segments, each starting with a letter. Default `com.j3.<name>`.", suggest: ['"com.example.game"'] },
      orientation: {
        doc: "Activity `screenOrientation`. Default `landscape`.",
        values: ["landscape", "portrait", "sensorLandscape", "sensorPortrait", "fullSensor", "unspecified"],
      },
      native_libs: { doc: "Folder of `<abi>/lib<name>.so` files shipped in the APK beside libmain.so (helper programs, prebuilt libs). At runtime they are in `J3_NATIVE_LIB_DIR`; run a program there with `j3.proc`.", suggest: ['"android/libs"'] },
    },
  },
  ui: {
    doc: "`.j3ui` components. Every file in `dir` and its subfolders compiles to one module, `<package>.ui`.",
    keys: {
      dir: { doc: "Folder of `.j3ui` files. Default `ui`.", suggest: ['"ui"'] },
      theme: { doc: "One global CSS file (scope 0), read at compile time.", suggest: ['"ui/theme.css"'] },
      lint: { doc: "`false` turns the `j3c check` CSS lint off.", values: BOOLS },
      "lint-off": { doc: "CSS lint rules `j3c check` skips (`j3c doc css`, Lint).", list: true, suggest: ['"never-matches"', '"duplicate-decl"', '"undefined-var"'] },
    },
  },
};

function register(context) {
  const diags = vscode.languages.createDiagnosticCollection("j3.toml");
  context.subscriptions.push(diags);

  const sel = { language: "j3toml" };
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(sel, {
      provideCompletionItems: (doc, pos) => completions(doc, pos),
    }),
    vscode.languages.registerHoverProvider(sel, {
      provideHover: (doc, pos) => hover(doc, pos),
    })
  );

  const refresh = (doc) => {
    if (doc.languageId === "j3toml") {
      diags.set(doc.uri, diagnostics(doc));
    }
  };
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(refresh),
    vscode.workspace.onDidChangeTextDocument((e) => refresh(e.document)),
    vscode.workspace.onDidCloseTextDocument((doc) => diags.delete(doc.uri))
  );
  vscode.workspace.textDocuments.forEach(refresh);
}

function completions(doc, pos) {
  const before = stripComment(doc.lineAt(pos.line).text.slice(0, pos.character));
  const section = sectionAt(doc, pos.line);

  const sec = /^\s*\[([A-Za-z0-9_-]*)$/.exec(before);
  if (sec) {
    return sectionItems(sec[1]);
  }

  const assigned = /^\s*([A-Za-z_][A-Za-z0-9_-]*)\s*=\s*(.*)$/.exec(before);
  if (assigned && SCHEMA[section]) {
    return valueItems(SCHEMA[section].keys[assigned[1]], assigned[2]);
  }

  if (before.includes("=")) {
    return [];
  }

  const key = /^\s*([A-Za-z_][A-Za-z0-9_-]*)$/.exec(before);
  if (section && SCHEMA[section] && (key || /^\s*$/.test(before))) {
    const prefix = key ? key[1] : "";
    return keyItems(doc, section, prefix, pos.line);
  }

  if (!section && /^\s*$/.test(before)) {
    return sectionItems("", true);
  }
  return [];
}

function sectionItems(prefix, bare) {
  return Object.keys(SCHEMA)
    .filter((name) => name.startsWith(prefix))
    .map((name) => {
      const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Module);
      item.detail = `[${name}]`;
      item.documentation = new vscode.MarkdownString(SCHEMA[name].doc);
      item.insertText = bare ? `[${name}]\n` : `${name}]`;
      return item;
    });
}

function keyItems(doc, section, prefix, line) {
  const used = keysInSection(doc, line);
  const spec = SCHEMA[section];
  return Object.keys(spec.keys)
    .filter((name) => name.startsWith(prefix) && !used.has(name))
    .map((name) => {
      const key = spec.keys[name];
      const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Field);
      item.detail = key.list ? "array" : key.values ? key.values.join(" | ") : "string";
      item.documentation = new vscode.MarkdownString(key.doc);
      item.insertText = key.list ? `${name} = []` : `${name} = `;
      return item;
    });
}

function valueItems(key, typed) {
  if (!key) {
    return [];
  }
  const choices = key.values || key.suggest || [];
  if (!choices.length) {
    return [];
  }
  const prefix = typed.replace(/^\s*\[?\s*"?/, "").replace(/"$/, "");
  return choices
    .filter((v) => v.replace(/"/g, "").startsWith(prefix))
    .map((v) => {
      const shown = v.replace(/"/g, "");
      const item = new vscode.CompletionItem(shown, vscode.CompletionItemKind.EnumMember);
      item.insertText = /^[A-Za-z0-9_.+-]+$/.test(shown) ? shown : `"${shown}"`;
      return item;
    });
}

function hover(doc, pos) {
  const line = stripComment(doc.lineAt(pos.line).text);
  const sectionLine = /^\s*\[([A-Za-z_][A-Za-z0-9_-]*)\]/.exec(line);
  if (sectionLine && SCHEMA[sectionLine[1]]) {
    const word = doc.getWordRangeAtPosition(pos);
    if (!word) {
      return null;
    }
    return new vscode.Hover(new vscode.MarkdownString(SCHEMA[sectionLine[1]].doc), word);
  }
  const keyLine = /^\s*([A-Za-z_][A-Za-z0-9_-]*)\s*=/.exec(line);
  const section = sectionAt(doc, pos.line);
  if (keyLine && SCHEMA[section] && SCHEMA[section].keys[keyLine[1]]) {
    const word = doc.getWordRangeAtPosition(pos);
    if (!word || doc.getText(word) !== keyLine[1]) {
      return null;
    }
    const key = SCHEMA[section].keys[keyLine[1]];
    const md = new vscode.MarkdownString(`**${section}.${keyLine[1]}**\n\n${key.doc}`);
    return new vscode.Hover(md, word);
  }
  return null;
}

function diagnostics(doc) {
  const out = [];
  let section = "";
  let sawPackage = false;
  let sawName = false;
  const seen = new Map();
  for (let i = 0; i < doc.lineCount; i++) {
    const stripped = stripComment(doc.lineAt(i).text);
    const pad = stripped.length - stripped.trimStart().length;
    const text = stripped.trim();
    const col = (n) => pad + n;
    if (!text) {
      continue;
    }
    const sec = /^\[([A-Za-z_][A-Za-z0-9_-]*)\]$/.exec(text);
    if (sec) {
      section = sec[1];
      if (!SCHEMA[section]) {
        out.push(warn(i, col(0), col(text.length), `unknown section \`${section}\``));
      } else if (section === "package") {
        sawPackage = true;
      }
      continue;
    }
    if (text.startsWith("[")) {
      out.push(warn(i, col(0), col(text.length), "expected `[section]`"));
      continue;
    }
    const eq = text.indexOf("=");
    if (eq < 0) {
      out.push(warn(i, col(0), col(text.length), "expected `key = value`"));
      continue;
    }
    const key = text.slice(0, eq).trim();
    const raw = text.slice(eq + 1).trim();
    if (!/^[A-Za-z_][A-Za-z0-9_-]*$/.test(key)) {
      out.push(warn(i, col(0), col(key.length), "bad key"));
      continue;
    }
    if (!section) {
      out.push(warn(i, col(0), col(key.length), "key outside a section"));
      continue;
    }
    const spec = SCHEMA[section];
    if (!spec) {
      continue;
    }
    if (!spec.keys[key]) {
      out.push(warn(i, col(0), col(key.length), `unknown key \`${key}\` in [${section}]`));
      continue;
    }
    const bag = seen.get(section) || new Set();
    if (bag.has(key)) {
      out.push(warn(i, col(0), col(key.length), `duplicate key \`${key}\``));
    }
    bag.add(key);
    seen.set(section, bag);
    if (section === "package" && key === "name" && unquote(raw)) {
      sawName = true;
    }
    const field = spec.keys[key];
    if (field.values && !valueOk(raw, field)) {
      out.push(
        warn(i, col(eq + 1), col(text.length), `\`${key}\` must be ${field.values.join(", ")}`)
      );
    }
  }
  if (doc.getText().trim() && (!sawPackage || !sawName)) {
    out.push(warn(0, 0, 1, "j3.toml needs [package] name"));
  }
  return out;
}

function valueOk(raw, field) {
  if (raw.startsWith("[")) {
    return field.list === true;
  }
  const v = unquote(raw);
  return field.values.includes(v);
}

function unquote(raw) {
  const t = raw.trim();
  if (t.startsWith('"') && t.endsWith('"') && t.length >= 2) {
    return t.slice(1, -1);
  }
  return t;
}

function sectionAt(doc, line) {
  for (let i = line; i >= 0; i--) {
    const text = stripComment(doc.lineAt(i).text).trim();
    const m = /^\[([A-Za-z_][A-Za-z0-9_-]*)\]/.exec(text);
    if (m) {
      return m[1];
    }
  }
  return "";
}

function keysInSection(doc, line) {
  const used = new Set();
  for (let i = line - 1; i >= 0; i--) {
    const text = stripComment(doc.lineAt(i).text).trim();
    if (/^\[/.test(text)) {
      break;
    }
    const m = /^([A-Za-z_][A-Za-z0-9_-]*)\s*=/.exec(text);
    if (m) {
      used.add(m[1]);
    }
  }
  return used;
}

function warn(line, start, end, message) {
  const diag = new vscode.Diagnostic(
    new vscode.Range(line, start, line, Math.max(end, start + 1)),
    message,
    vscode.DiagnosticSeverity.Warning
  );
  diag.source = "j3.toml";
  return diag;
}

function stripComment(line) {
  let inStr = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') {
      inStr = !inStr;
    } else if (c === "\\" && inStr) {
      i++;
    } else if (c === "#" && !inStr) {
      return line.slice(0, i);
    }
  }
  return line;
}

module.exports = { register };

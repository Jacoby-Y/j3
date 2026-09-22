// LSP client. Spawns `j3c lsp` (same parser + typed IR as the compiler).
const vscode = require("vscode");
const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");
const toml = require("./toml");

function activate(context) {
  console.log("J3: activating, j3c =", j3cPath());
  const client = new Client();
  client.start(context);
  context.subscriptions.push({ dispose: () => client.stop() });
  toml.register(context);

  context.subscriptions.push(
    vscode.commands.registerCommand("j3.run", () =>
      task(["run", folder()])
    ),
    vscode.commands.registerCommand("j3.build", () =>
      task(["build", folder()])
    ),
    vscode.commands.registerCommand("j3.fmt", () =>
      task(["fmt", "--write", folder()])
    )
  );
}

function deactivate() {}

function completionKind(kind) {
  const k = vscode.CompletionItemKind;
  switch (kind) {
    case 2:
      return k.Method;
    case 3:
      return k.Function;
    case 5:
      return k.Field;
    case 6:
      return k.Variable;
    case 9:
      return k.Module;
    case 13:
      return k.Enum;
    case 14:
      return k.Keyword;
    case 15:
      return k.Snippet;
    case 20:
      return k.EnumMember;
    case 21:
      return k.Constant;
    case 22:
      return k.Struct;
    default:
      return k.Text;
  }
}

function toCompletion(it) {
  const item = new vscode.CompletionItem(it.label, completionKind(it.kind));
  if (it.detail) {
    item.detail = it.detail;
  }
  if (it.documentation) {
    const text = it.documentation.value || it.documentation;
    item.documentation = new vscode.MarkdownString(String(text));
  }
  if (it.insertText) {
    item.insertText = it.insertTextFormat === 2
      ? new vscode.SnippetString(it.insertText)
      : it.insertText;
  }
  if (it.sortText) {
    item.sortText = it.sortText;
  }
  if (it.commitCharacters) {
    item.commitCharacters = it.commitCharacters;
  }
  return item;
}

function j3cPath() {
  const cfg = vscode.workspace.getConfiguration("j3");
  const p = cfg.get("j3cPath");
  if (p && p !== "j3c") {
    return p;
  }
  const folders = vscode.workspace.workspaceFolders;
  if (folders && folders.length) {
    const cand = path.join(folders[0].uri.fsPath, "j3c", "target", "debug", "j3c");
    try {
      if (fs.existsSync(cand)) {
        return cand;
      }
    } catch (_) {}
  }
  return "j3c";
}

function sh(p) {
  if (/[\s"'\\$]/.test(p)) {
    return `"${String(p).replace(/"/g, '\\"')}"`;
  }
  return p;
}

function task(args) {
  const cmd = [sh(j3cPath()), ...args.map(sh)].join(" ");
  const t = new vscode.Task(
    { type: "j3" },
    vscode.TaskScope.Workspace,
    cmd,
    "j3",
    new vscode.ShellExecution(cmd)
  );
  return vscode.tasks.executeTask(t);
}

function folder() {
  const f = vscode.workspace.workspaceFolders;
  return f && f[0] ? f[0].uri.fsPath : ".";
}

function rangeOf(r) {
  return new vscode.Range(
    r.start.line,
    r.start.character,
    r.end.line,
    r.end.character
  );
}

class Client {
  constructor() {
    this.child = null;
    this.id = 1;
    this.pending = new Map();
    this.buf = Buffer.alloc(0);
    this.diags = vscode.languages.createDiagnosticCollection("j3");
    this.stopping = false;
  }

  start(context) {
    this.spawn();
    const add = (d) => context.subscriptions.push(d);

    add(vscode.workspace.onDidOpenTextDocument((d) => this.open(d)));
    add(vscode.workspace.onDidChangeTextDocument((e) => this.change(e)));
    add(vscode.workspace.onDidCloseTextDocument((d) => this.close(d)));

    add(vscode.languages.registerHoverProvider("j3", {
      provideHover: async (doc, pos) => {
        const r = await this.send("textDocument/hover", {
          textDocument: { uri: doc.uri.toString() },
          position: { line: pos.line, character: pos.character },
        });
        if (!r || !r.contents) {
          return null;
        }
        const md = new vscode.MarkdownString(r.contents.value || String(r.contents));
        return new vscode.Hover(md);
      },
    }));
    add(vscode.languages.registerDefinitionProvider("j3", {
      provideDefinition: async (doc, pos) => {
        const r = await this.send("textDocument/definition", {
          textDocument: { uri: doc.uri.toString() },
          position: { line: pos.line, character: pos.character },
        });
        if (!r || !r.uri) {
          return null;
        }
        return new vscode.Location(vscode.Uri.parse(r.uri), rangeOf(r.range));
      },
    }));
    add(vscode.languages.registerCompletionItemProvider(
      "j3",
      {
        provideCompletionItems: async (doc, pos) => {
          const r = await this.send("textDocument/completion", {
            textDocument: { uri: doc.uri.toString() },
            position: { line: pos.line, character: pos.character },
          });
          const items = r && r.items ? r.items : [];
          return items.map(toCompletion);
        },
      },
      ".",
      "{",
      "@",
      "["
    ));
    add(vscode.languages.registerSignatureHelpProvider(
      "j3",
      {
        provideSignatureHelp: async (doc, pos) => {
          const r = await this.send("textDocument/signatureHelp", {
            textDocument: { uri: doc.uri.toString() },
            position: { line: pos.line, character: pos.character },
          });
          if (!r || !r.signatures || !r.signatures.length) {
            return null;
          }
          const help = new vscode.SignatureHelp();
          help.signatures = r.signatures.map((s) => {
            const sig = new vscode.SignatureInformation(s.label);
            sig.parameters = (s.parameters || []).map(
              (p) => new vscode.ParameterInformation(p.label)
            );
            if (s.documentation) {
              const text = s.documentation.value || s.documentation;
              sig.documentation = new vscode.MarkdownString(String(text));
            }
            return sig;
          });
          help.activeSignature = r.activeSignature || 0;
          help.activeParameter = r.activeParameter || 0;
          return help;
        },
      },
      "(",
      ","
    ));
    add(vscode.languages.registerDocumentSymbolProvider("j3", {
      provideDocumentSymbols: async (doc) => {
        const r = await this.send("textDocument/documentSymbol", {
          textDocument: { uri: doc.uri.toString() },
        });
        if (!Array.isArray(r)) {
          return [];
        }
        return r.map((s) => {
          const loc = s.location || s;
          return new vscode.SymbolInformation(
            s.name,
            s.kind,
            rangeOf(loc.range),
            doc.uri,
            s.containerName || ""
          );
        });
      },
    }));
    add(vscode.languages.registerCodeActionsProvider("j3", {
      provideCodeActions: async (doc, range, context) => {
        const r = await this.send("textDocument/codeAction", {
          textDocument: { uri: doc.uri.toString() },
          range: {
            start: { line: range.start.line, character: range.start.character },
            end: { line: range.end.line, character: range.end.character },
          },
          context: {
            diagnostics: context.diagnostics.map((d) => ({
              message: d.message,
              range: {
                start: { line: d.range.start.line, character: d.range.start.character },
                end: { line: d.range.end.line, character: d.range.end.character },
              },
            })),
          },
        });
        if (!Array.isArray(r)) {
          return [];
        }
        return r.map((a) => {
          const action = new vscode.CodeAction(a.title, vscode.CodeActionKind.QuickFix);
          const edit = new vscode.WorkspaceEdit();
          const changes = a.edit && a.edit.changes;
          if (changes) {
            for (const uri of Object.keys(changes)) {
              for (const e of changes[uri]) {
                edit.replace(vscode.Uri.parse(uri), rangeOf(e.range), e.newText);
              }
            }
          }
          action.edit = edit;
          return action;
        });
      },
    }));
    add(vscode.languages.registerDocumentFormattingEditProvider("j3", {
      provideDocumentFormattingEdits: async (doc) => {
        const r = await this.send("textDocument/formatting", {
          textDocument: { uri: doc.uri.toString() },
          options: { tabSize: 4, insertSpaces: true },
        });
        if (!Array.isArray(r)) {
          return [];
        }
        return r.map((e) => vscode.TextEdit.replace(rangeOf(e.range), e.newText));
      },
    }));
  }

  spawn() {
    if (this.child) {
      this.child.removeAllListeners();
      this.child.kill();
    }
    this.buf = Buffer.alloc(0);
    this.pending.forEach((fn) => fn(null));
    this.pending.clear();
    this.startedAt = Date.now();
    const bin = j3cPath();
    this.child = spawn(bin, ["lsp"], { stdio: ["pipe", "pipe", "pipe"] });
    this.child.on("error", (err) => {
      vscode.window.showErrorMessage(`J3: could not start ${bin} (${err.message})`);
    });
    this.child.stderr.on("data", (d) => console.error(String(d)));
    this.child.stdout.on("data", (d) => this.onData(d));
    this.child.on("exit", () => {
      if (this.stopping) {
        return;
      }
      if (Date.now() - this.startedAt < 1000) {
        vscode.window.showErrorMessage(
          `J3: language server exited immediately (${bin}). Set j3.j3cPath if that binary is missing.`
        );
        return;
      }
      setTimeout(() => {
        if (!this.stopping) {
          this.spawn();
        }
      }, 400);
    });
    const root = vscode.workspace.workspaceFolders
      ? vscode.workspace.workspaceFolders[0].uri.toString()
      : null;
    this.send("initialize", {
      processId: process.pid,
      rootUri: root,
      capabilities: {},
    }).then(() => {
      this.notify("initialized", {});
      vscode.workspace.textDocuments.forEach((d) => this.open(d));
    });
  }

  open(doc) {
    if (doc.languageId !== "j3") {
      return;
    }
    this.notify("textDocument/didOpen", {
      textDocument: {
        uri: doc.uri.toString(),
        languageId: "j3",
        version: doc.version,
        text: doc.getText(),
      },
    });
  }

  change(e) {
    if (e.document.languageId !== "j3") {
      return;
    }
    this.notify("textDocument/didChange", {
      textDocument: { uri: e.document.uri.toString(), version: e.document.version },
      contentChanges: [{ text: e.document.getText() }],
    });
  }

  close(doc) {
    if (doc.languageId !== "j3") {
      return;
    }
    this.notify("textDocument/didClose", {
      textDocument: { uri: doc.uri.toString() },
    });
  }

  send(method, params) {
    const id = this.id++;
    this.write({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          resolve(null);
        }
      }, 20000);
    });
  }

  notify(method, params) {
    this.write({ jsonrpc: "2.0", method, params });
  }

  write(obj) {
    if (!this.child || !this.child.stdin.writable) {
      return;
    }
    const body = JSON.stringify(obj);
    this.child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  }

  onData(chunk) {
    this.buf = Buffer.concat([this.buf, chunk]);
    while (true) {
      const headEnd = this.buf.indexOf("\r\n\r\n");
      if (headEnd < 0) {
        return;
      }
      const header = this.buf.slice(0, headEnd).toString("utf8");
      const m = /Content-Length:\s*(\d+)/i.exec(header);
      if (!m) {
        this.buf = this.buf.slice(headEnd + 4);
        continue;
      }
      const len = parseInt(m[1], 10);
      const start = headEnd + 4;
      if (this.buf.length < start + len) {
        return;
      }
      const body = this.buf.slice(start, start + len).toString("utf8");
      this.buf = this.buf.slice(start + len);
      this.onMessage(JSON.parse(body));
    }
  }

  onMessage(msg) {
    if (msg.method === "textDocument/publishDiagnostics") {
      const uri = vscode.Uri.parse(msg.params.uri);
      const items = (msg.params.diagnostics || []).map((d) => {
        const item = new vscode.Diagnostic(
          rangeOf(d.range),
          d.message,
          vscode.DiagnosticSeverity.Error
        );
        item.source = d.source || "j3c";
        return item;
      });
      this.diags.set(uri, items);
      return;
    }
    if (msg.id != null && this.pending.has(msg.id)) {
      const fn = this.pending.get(msg.id);
      this.pending.delete(msg.id);
      fn(msg.result);
    }
  }

  stop() {
    this.stopping = true;
    if (this.child) {
      this.send("shutdown", null).then(() => this.notify("exit", null));
      this.child.kill();
    }
    this.diags.dispose();
  }
}

module.exports = { activate, deactivate };

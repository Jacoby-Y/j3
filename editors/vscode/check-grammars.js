// Offline grammar check (no deps): every grammar parses, every `include` resolves, every regex
// compiles, and a few lines of .j3ui hit the rules they should.
// Run: node check-grammars.js
const fs = require("fs");
const path = require("path");
const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, "package.json"), "utf8"));
const scopes = new Set(pkg.contributes.grammars.map((g) => g.scopeName));
const external = new Set(["source.css"]); // shipped by VS Code
let bad = 0;
const fail = (m) => { console.error("FAIL " + m); bad++; };

for (const g of pkg.contributes.grammars) {
  const file = path.join(__dirname, g.path);
  const gr = JSON.parse(fs.readFileSync(file, "utf8"));
  if (gr.scopeName !== g.scopeName) fail(`${g.path}: scopeName ${gr.scopeName}`);
  const walk = (node, where) => {
    if (Array.isArray(node)) return node.forEach((n, i) => walk(n, where + "[" + i + "]"));
    if (!node || typeof node !== "object") return;
    if (node.include) {
      const inc = node.include;
      if (inc.startsWith("#")) {
        if (!gr.repository || !gr.repository[inc.slice(1)]) fail(`${g.path} ${where}: include ${inc} unresolved`);
      } else if (inc !== "$self" && inc !== "$base" && !scopes.has(inc) && !external.has(inc)) {
        fail(`${g.path} ${where}: include ${inc} unresolved`);
      }
    }
    for (const k of ["match", "begin", "end", "while"]) {
      if (typeof node[k] === "string") {
        try { new RegExp(node[k]); } catch (e) { fail(`${g.path} ${where}.${k}: ${e.message}`); }
      }
    }
    for (const [k, v] of Object.entries(node)) walk(v, where + "." + k);
  };
  walk(gr, g.path);
  console.log("ok   " + g.path);
}

// .j3ui rules
const ui = JSON.parse(fs.readFileSync(path.join(__dirname, "syntaxes/j3ui.tmLanguage.json"), "utf8")).repository;
const hits = (re, text) => new RegExp(re).test(text);
const cases = [
  [ui.script.begin, "<script>"],
  [ui.style.begin, "<style>"],
  [ui.block.begin, "{#if canBuy()}"],
  [ui.block.begin, "{:else}"],
  [ui.block.begin, "{#each xs as x, i}"],
  [ui.expr.begin, "{sim.gold}"],
  [ui.tag.patterns[0].begin, "<UpgradeRow label={a}"],
  [ui.tag.patterns[1].begin, "<button disabled={a}"],
  [ui.tag.patterns[2].begin, "<blink>"],
  [ui.attrs.patterns[0].match, "on:click={x}"],
  [ui.comment.begin, "<!-- c -->"],
];
for (const [re, text] of cases) if (!hits(re, text)) fail(`/${re}/ should match ${JSON.stringify(text)}`);
if (hits(ui.tag.patterns[1].begin, "<Blink")) fail("element rule matched a component");
if (hits(ui.tag.patterns[1].begin, "<blink")) fail("element rule matched an unknown element");
if (!bad) console.log("ok   j3ui rules");
process.exit(bad ? 1 : 0);

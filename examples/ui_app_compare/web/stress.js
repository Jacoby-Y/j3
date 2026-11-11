// The stress scene: N cards, every one changes text, bar width and (staggered) class each frame.
// Warm up 2 s, count frames for 8 s, report `mark` and `done frames=N` to the host (../stress.py reads them).
const N = 1200;
const grid = document.getElementById("grid");
const cards = [];
for (let i = 0; i < N; i++) {
  const card = document.createElement("div");
  const span = document.createElement("span");
  const bar = document.createElement("div");
  const fill = document.createElement("div");
  bar.className = "bar";
  fill.className = "fill";
  bar.append(fill);
  card.append(span, bar);
  grid.append(card);
  cards.push({ card, span, fill });
}

function report(line) {
  if (window.__TAURI__) window.__TAURI__.core.invoke("report", { line });
  else if (window.go) window.go.main.App.Report(line);
  else if (window.electronReport) window.electronReport(line);
}

let frame = 0, marked = 0, phase = 0;
const t0 = performance.now();
function tick() {
  frame++;
  for (let i = 0; i < N; i++) {
    const c = cards[i];
    c.card.className = "card c" + (Math.floor((i + frame) / 8) % 4);
    c.span.textContent = (i * 7 + frame) % 1000;
    c.fill.style.width = ((i + frame) % 100) + "%";
  }
  const t = performance.now() - t0;
  if (phase === 0 && t >= 2000) { phase = 1; marked = frame; report("mark"); }
  else if (phase === 1 && t >= 10000) { phase = 2; report(`done frames=${frame - marked}`); return; }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

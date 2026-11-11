let nextId = 1;
const items = [];
const push = (text) => items.push({ id: nextId++, text, done: false });
push("Write the app");
push("Measure the build");

const list = document.getElementById("list");
const foot = document.getElementById("foot");
const draft = document.getElementById("draft");

function render() {
  list.replaceChildren(...items.map((it) => {
    const row = document.createElement("div");
    row.className = "row" + (it.done ? " done" : "");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = it.done;
    box.onchange = () => { it.done = box.checked; render(); };
    const text = document.createElement("span");
    text.className = "text";
    text.textContent = it.text;
    const x = document.createElement("button");
    x.textContent = "×";
    x.onclick = () => { items.splice(items.indexOf(it), 1); render(); };
    row.append(box, text, x);
    return row;
  }));
  foot.textContent = `${items.filter((it) => !it.done).length} left`;
}

document.getElementById("add").onsubmit = (e) => {
  e.preventDefault();
  if (draft.value) { push(draft.value); draft.value = ""; render(); }
};
render();

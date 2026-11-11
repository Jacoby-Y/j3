const { app, BrowserWindow } = require("electron");
const path = require("path");

app.whenReady().then(() => {
  const win = new BrowserWindow({ width: 480, height: 520, title: "Todos" });
  win.setMenu(null);
  win.loadFile(path.join(__dirname, "web", "index.html"));
});
app.on("window-all-closed", () => app.quit());

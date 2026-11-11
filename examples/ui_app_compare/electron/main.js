const { app, BrowserWindow, ipcMain } = require("electron");
const path = require("path");
const stress = !!process.env.STRESS;

if (stress) {
  // measure how fast it can go, not the display refresh
  app.commandLine.appendSwitch("disable-frame-rate-limit");
  app.commandLine.appendSwitch("disable-gpu-vsync");
  // the app keeps running after `done`: ../stress.py reads its CPU time, then stops it
  ipcMain.on("report", (_e, line) => console.log(line));
}

app.whenReady().then(() => {
  const win = new BrowserWindow({
    width: stress ? 1280 : 480,
    height: stress ? 800 : 520,
    useContentSize: true,
    title: "Todos",
    webPreferences: stress ? { preload: path.join(__dirname, "preload.js"), backgroundThrottling: false } : {},
  });
  win.setMenu(null);
  win.loadFile(path.join(__dirname, "web", stress ? "stress.html" : "index.html"));
});
app.on("window-all-closed", () => app.quit());

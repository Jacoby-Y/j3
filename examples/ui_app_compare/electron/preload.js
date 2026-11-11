// Stress runs only: lets the page send its `mark` / `done` lines to the main process.
const { contextBridge, ipcRenderer } = require("electron");
contextBridge.exposeInMainWorld("electronReport", (line) => ipcRenderer.send("report", line));

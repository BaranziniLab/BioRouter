const { app, BrowserWindow } = require('electron');

app.whenReady().then(() => {
  const harnessUrl = new URL(process.env.BIOROUTER_PREVIEW_HARNESS_URL || '');
  if (harnessUrl.protocol !== 'http:' || harnessUrl.hostname !== '127.0.0.1') {
    throw new Error('preview harness only loads its loopback Vite server');
  }
  const window = new BrowserWindow({
    width: 1400,
    height: 1000,
    show: true,
    webPreferences: {
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  void window.loadURL(harnessUrl.toString());
});

app.on('window-all-closed', () => app.quit());

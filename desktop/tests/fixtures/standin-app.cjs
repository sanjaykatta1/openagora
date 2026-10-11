// A stand-in for another app on the computer (say, Firefox): one window
// filled with a known colour. Prints its window id, as Electron names it, so
// the test can put it "in front".
const { app, BrowserWindow } = require('electron');

app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 400, height: 300, x: 40, y: 40, backgroundColor: '#e04020', show: true, webPreferences: { sandbox: true } });
  await window.loadURL('data:text/html,<body style="margin:0;background:%23e04020"></body>');
  process.stdout.write(`${window.getMediaSourceId()}\n`);
});

// A stand-in for another app on the computer (say, Firefox): one window
// filled with a known colour. Prints its window id, as Electron names it, so
// the test can put it "in front". A line "title <text>" on stdin retitles the
// window (as when the user opens a private window); it answers "ok".
const { app, BrowserWindow } = require('electron');
const readline = require('node:readline');

app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 400, height: 300, x: 40, y: 40, title: 'Docs — Mozilla Firefox', backgroundColor: '#e04020', show: true, webPreferences: { sandbox: true } });
  window.on('page-title-updated', (event) => event.preventDefault());
  await window.loadURL('data:text/html,<body style="margin:0;background:%23e04020"></body>');
  process.stdout.write(`${window.getMediaSourceId()}\n`);
  readline.createInterface({ input: process.stdin }).on('line', (line) => {
    if (line.startsWith('title ')) window.setTitle(line.slice(6));
    process.stdout.write('ok\n');
  });
});

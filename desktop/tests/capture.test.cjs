// Sunday's shortcut setting, and what the window accepts as a capture request
// from the engine.
const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { DEFAULT_DESKTOP_SETTINGS, readDesktopSettings, saveDesktopSettings, validShortcut } = require('../dist/electron/desktop-settings');
const { parseTarget } = require('../dist/electron/engine-link');

function profile(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-desktop-settings-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return directory;
}

test('a shortcut is modifiers and one key', () => {
  for (const ok of ['CommandOrControl+Shift+Space', 'Alt+F2', 'Ctrl+Shift+.', 'Super+K', 'CmdOrCtrl+Alt+Plus']) {
    assert.equal(validShortcut(ok), true, ok);
  }
  for (const bad of ['Space', 'Shift+', 'Ctrl+Ctrl+K', 'Hyper+K', 'CommandOrControl+Space+X', '', 42, 'Ctrl+' + 'K'.repeat(70)]) {
    assert.equal(validShortcut(bad), false, String(bad));
  }
});

test('settings start with the default shortcut and keep running in the tray', (t) => {
  const directory = profile(t);
  assert.deepEqual(readDesktopSettings(directory), DEFAULT_DESKTOP_SETTINGS);
  assert.equal(DEFAULT_DESKTOP_SETTINGS.keepRunning, true);
  const saved = saveDesktopSettings(directory, { version: 1, shortcut: 'Alt+Shift+S', keepRunning: false });
  assert.deepEqual(readDesktopSettings(directory), saved);
  assert.throws(() => saveDesktopSettings(directory, { version: 1, shortcut: 'S', keepRunning: true }), /Invalid settings/);
  assert.throws(() => saveDesktopSettings(directory, { version: 1, shortcut: 'Alt+S', keepRunning: 'yes' }), /Invalid settings/);
  assert.deepEqual(readDesktopSettings(directory), saved);
  // A damaged file falls back to the defaults, field by field.
  fs.writeFileSync(path.join(directory, 'desktop-settings.json'), JSON.stringify({ shortcut: 'nonsense', keepRunning: false }));
  assert.deepEqual(readDesktopSettings(directory), { version: 1, shortcut: DEFAULT_DESKTOP_SETTINGS.shortcut, keepRunning: false });
  fs.writeFileSync(path.join(directory, 'desktop-settings.json'), '{');
  assert.deepEqual(readDesktopSettings(directory), DEFAULT_DESKTOP_SETTINGS);
});

test('the window captures only a window by its id, an app tab, or a window the user picks', () => {
  assert.deepEqual(parseTarget({ kind: 'window', id: 4211, app: 'Firefox' }), { kind: 'window', id: 4211, app: 'Firefox' });
  assert.deepEqual(parseTarget({ kind: 'tab', app: 't3code' }), { kind: 'tab', app: 't3code' });
  assert.deepEqual(parseTarget({ kind: 'pick' }), { kind: 'pick' });
  for (const bad of [
    null, 'window', { kind: 'window', id: '4211' }, { kind: 'window', id: -1 }, { kind: 'window', id: 1.5 },
    { kind: 'tab', app: '../etc' }, { kind: 'tab' }, { kind: 'screen', id: 0 }, { kind: 'file', path: '/etc/passwd' },
  ]) {
    assert.equal(parseTarget(bad), null, JSON.stringify(bad));
  }
});

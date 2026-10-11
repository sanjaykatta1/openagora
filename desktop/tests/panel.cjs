// Sunday's shortcut and panel (acceptance tests 6 and 8, the window's side):
// the shortcut opens the panel over the app the user is in, the panel says
// what Sunday may see and lets the user allow it, OpenAgora's window captures
// what the engine asks for (an app's tab, or any window by its OS id), the
// clipboard goes on the desk, and Settings shows the shortcut, the allowed
// apps and the never-capture list. The engine is a stand-in; Rust tests
// cover its side.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
let daemon;
let dashboard;
const directory = tempProfile('openagora-panel-');
const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** Sunday's panel window, once it has opened. */
async function panelPage(app) {
  let found = null;
  await waitFor(() => (found = app.windows().find((w) => w.url().endsWith('#panel'))), 'the Sunday panel to open');
  await found.waitForLoadState('domcontentloaded');
  return found;
}

const panelVisible = (app) => app.evaluate(() => globalThis.openagoraTest.panel()?.isVisible() ?? false);
const press = (app) => app.evaluate(() => globalThis.openagoraTest.pressShortcut());

async function run() {
  dashboard = await fakeDashboard();
  daemon = await fakeDaemon();
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  const notes = {
    id: 'notes', name: 'Web notes', summary: 'A notes app', category: 'productivity',
    homepage: 'https://example.com', source: 'https://github.com/example/notes', license: 'MIT',
    platforms: ['macos', 'linux', 'windows'], available: true, role: 'app', profile: null, ui: 'web',
    steps: [], permissions: [], installed: true, adopted: false, present: true, status: 'running',
    pid: 42, url: dashboard.url, home_url: dashboard.url,
  };
  session = await launch({
    directory, daemon,
    fake: freshFakeState({
      present: true, installed: true, clean: true, setup: 'ready', running: true, pid: 7, connected: true,
      dashboardUrl: dashboard.url, extraApps: [notes],
      grants: [{ client: 'sunday', source: 'firefox', source_name: 'Firefox', destination: 'sunday', type: 'image/*', scope: 'always', granted_at: 1 }],
    }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');

  // The window tells the engine which process and windows are OpenAgora's.
  const hello = daemon.hellos.at(-1);
  assert.equal(typeof hello.pid, 'number');
  assert.match(hello.main, /^window:\d+:\d+$/);

  // The user opens the Notes tab: the engine hears which app the window shows.
  await page.locator('webview').first().waitFor({ state: 'attached' });
  daemon.send({ method: 'app/open', params: { id: 'notes' } });
  await page.getByRole('button', { name: 'Web notes', exact: true }).waitFor();
  await waitFor(() => daemon.received.some((m) => m.method === 'focus/openagora' && m.params.tab === 'notes'), 'the window to report the Notes tab');

  // The engine's side of the shortcut: Notes isn't allowed yet. Capturing
  // asks the window for a picture of the Notes tab.
  const captureCalls = [];
  const pictures = [];
  let verdict = 'not-allowed';
  const target = { kind: 'tab', app: 'Web notes', key: 'openagora:notes' };
  daemon.handle('shortcut/pressed', () => ({ target, verdict, rule: verdict === 'never' ? '1Password' : null, unknown: null, item: null, error: null }));
  daemon.handle('shortcut/capture', async (params) => {
    captureCalls.push(params);
    const reply = await daemon.ask('capture/window', { target: { kind: 'tab', app: 'notes' }, max: 1920 });
    assert.ok(reply.result, JSON.stringify(reply));
    pictures.push(reply.result);
    return { target, verdict: 'allowed', rule: null, unknown: null, item: { id: 'd-0123456789', description: 'screenshot of Web notes' }, error: null };
  });

  // Pressing the shortcut opens Sunday's panel, over the app, with Sunday's chat.
  await press(app);
  const panel = await panelPage(app);
  await panel.getByText('Sunday can’t see Web notes yet.').waitFor();
  assert.equal(await panelVisible(app), true);
  assert.equal(await app.evaluate(() => globalThis.openagoraTest.panel().isAlwaysOnTop()), true);
  await waitFor(() => dashboard.requests.includes('/chat?profile=sunday'), 'Sunday’s chat in the panel');
  assert.match(await panel.locator('body').innerText(), /sent to the model provider/);

  // Allowing it captures the tab; the panel shows what Sunday got.
  await panel.getByRole('button', { name: 'Always allow' }).click();
  await panel.getByText('Sunday sees Web notes.').waitFor();
  assert.deepEqual(captureCalls, [{ allow: 'always' }]);
  const picture = Buffer.from(pictures[0].data, 'base64');
  assert.ok(picture.subarray(0, 8).equals(PNG), 'the tab capture is a PNG');
  assert.ok(pictures[0].width > 0 && pictures[0].height > 0);
  assert.equal(await panel.locator('img.seeing-picture').count(), 1);

  // A question while the panel is open is asked in the panel, with its choices.
  daemon.send({ id: 'ask-1', method: 'confirm/ask', params: {
    client: 'sunday', title: 'Let Sunday see Firefox?', message: 'Sunday asks to see Firefox.',
    choices: [{ id: 'once', label: 'Allow once' }, { id: 'always', label: 'Always allow' }],
  } });
  const dialog = panel.getByRole('alertdialog', { name: 'Let Sunday see Firefox?' });
  await dialog.waitFor();
  assert.equal(await panel.evaluate(() => document.activeElement?.textContent), 'Don’t allow');
  await dialog.getByRole('button', { name: 'Allow once' }).click();
  assert.deepEqual((await daemon.reply('ask-1')).result, { allowed: true, choice: 'once' });

  // The clipboard goes on the desk for Sunday.
  const deskAdds = [];
  daemon.handle('desk/add', (params) => { deskAdds.push(params); return { id: 'd-aaaaaaaaaa' }; });
  await app.evaluate(({ clipboard }) => clipboard.writeText('Remember the milk'));
  await panel.getByRole('button', { name: 'Add clipboard' }).click();
  await panel.getByRole('status').filter({ hasText: 'Put the clipboard on the desk for Sunday.' }).waitFor();
  assert.deepEqual(deskAdds, [{ kind: 'text', text: 'Remember the milk', source: { app: 'Clipboard' } }]);

  // Escape closes the panel; the shortcut brings it back. A never-captured
  // app is said so, and nothing is captured.
  await panel.keyboard.press('Escape');
  await waitFor(async () => !(await panelVisible(app)), 'the panel to close');
  verdict = 'never';
  await press(app);
  await panel.getByText('OpenAgora never captures Web notes.').waitFor();
  assert.equal(captureCalls.length, 1);

  // OpenAgora's window also captures any window by its OS id, through the
  // OS's own window capture: here, its own main window.
  if (process.platform === 'linux') {
    const id = Number(daemon.hellos.at(-1).main.split(':')[1]);
    const reply = await daemon.ask('capture/window', { target: { kind: 'window', id }, max: 640 }, 60000);
    assert.ok(reply.result, JSON.stringify(reply.error));
    assert.ok(Buffer.from(reply.result.data, 'base64').subarray(0, 8).equals(PNG));
    assert.ok(reply.result.width <= 640 && reply.result.width > 100);
  }
  // A target the window doesn't know is refused.
  const bad = await daemon.ask('capture/window', { target: { kind: 'file', path: '/etc/passwd' } });
  assert.match(bad.error.message, /unknown capture target/);

  // Settings → What Sunday can see.
  await page.bringToFront();
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'What Sunday can see');
  const shortcut = page.getByRole('textbox', { name: 'Sunday’s shortcut' });
  assert.equal(await shortcut.inputValue(), 'CommandOrControl+Shift+Space');
  assert.match(await page.locator('body').innerText(), /1Password, Private browser windows/);
  // Allowed apps can be taken back.
  await page.getByRole('listitem').filter({ hasText: 'Firefox' }).getByRole('button', { name: 'Remove' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday may no longer see Firefox.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['broker-revoke', 'firefox']);
  // The never-capture list takes the user's own apps and window titles.
  await page.getByLabel('An app').fill('Signal');
  await page.getByRole('button', { name: 'Never capture' }).click();
  await page.getByRole('status').filter({ hasText: 'OpenAgora will never capture Signal.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['never-add', { name: 'Signal', app: 'Signal' }]);
  await page.getByRole('listitem').filter({ hasText: 'Signal' }).getByRole('button', { name: 'Remove' }).click();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['never-remove', 'Signal']);
  // The shortcut can be changed; a shortcut without a key isn't saved.
  await shortcut.fill('Shift+');
  await page.getByRole('button', { name: 'Save shortcut' }).click();
  await page.getByRole('alert').filter({ hasText: /Invalid settings/ }).waitFor();
  await shortcut.fill('CommandOrControl+Alt+K');
  await page.getByRole('button', { name: 'Save shortcut' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday’s shortcut is now Ctrl + Alt + K.' }).waitFor();
  const saved = JSON.parse(fs.readFileSync(path.join(directory, 'desktop-settings.json'), 'utf8'));
  assert.deepEqual(saved, { version: 1, shortcut: 'CommandOrControl+Alt+K', keepRunning: true });
  assert.equal(await app.evaluate(() => globalThis.openagoraTest.shortcutRegistered()), true);
  // Keeping OpenAgora in the tray is on by default, and can be turned off.
  const keep = page.getByRole('checkbox', { name: /Keep OpenAgora running/ });
  assert.equal(await keep.isChecked(), true);
  await keep.uncheck();
  await page.getByRole('status').filter({ hasText: 'OpenAgora closes with its window.' }).waitFor();
  assert.equal(JSON.parse(fs.readFileSync(path.join(directory, 'desktop-settings.json'), 'utf8')).keepRunning, false);
  await keep.check();

  // Closing the window keeps OpenAgora running, so the shortcut still works.
  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('#panel') && w.webContents.getURL().endsWith('index.html'))?.close());
  await new Promise((r) => setTimeout(r, 500));
  verdict = 'not-allowed';
  await press(app);
  await panel.getByText('Sunday can’t see Web notes yet.').waitFor();
  assert.deepEqual(session.errors, []);
  console.log('Panel UI passed: shortcut, panel over the app, allow, tab and window capture, choices, clipboard, never, settings, keep running.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

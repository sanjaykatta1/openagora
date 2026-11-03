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
  const pressed = () => ({ target, verdict, rule: verdict === 'never' ? '1Password' : null, unknown: null, pick: false, item: null, error: null });
  daemon.handle('shortcut/pressed', pressed);
  daemon.handle('shortcut/capture', async (params) => {
    captureCalls.push(params);
    const reply = await daemon.ask('capture/window', { target: { kind: 'tab', app: 'notes' }, max: 1920, purpose: 'shortcut' });
    assert.ok(reply.result, JSON.stringify(reply));
    pictures.push(reply.result);
    return { target, verdict: 'allowed', rule: null, unknown: null, item: { id: 'd-0123456789', description: 'screenshot of Web notes' }, error: null };
  });

  // Pressing the shortcut opens Sunday's panel, over the app, with Sunday's chat.
  await press(app);
  const panel = await panelPage(app);
  await panel.getByText('Sunday can’t see Web notes yet.').waitFor();
  assert.equal(await panelVisible(app), true);
  assert.match(await panel.locator('.bar').innerText(), /^Sunday/);
  assert.equal(await app.evaluate(() => globalThis.openagoraTest.panel().isAlwaysOnTop()), true);
  await waitFor(() => dashboard.requests.includes('/chat?profile=sunday'), 'Sunday’s chat in the panel');
  assert.match(await panel.locator('body').innerText(), /sent to its model provider/);

  // Allowing it captures the tab; the panel shows what Sunday got.
  await panel.getByRole('button', { name: 'Always allow' }).click();
  await panel.getByText('Sunday sees Web notes.').waitFor();
  // The panel says which app the user allowed.
  assert.deepEqual(captureCalls, [{ allow: 'always', key: 'openagora:notes' }]);
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
  // Dropped files go on the desk as their contents, read by the panel: the
  // window never reads a file by a path a page gives it.
  await panel.evaluate(() => {
    const data = new DataTransfer();
    data.items.add(new File(['hello desk'], 'notes.txt', { type: 'text/plain' }));
    document.querySelector('.panel-shell').dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }));
  });
  await panel.getByRole('status').filter({ hasText: 'Put the file on the desk for Sunday.' }).waitFor();
  assert.deepEqual(deskAdds.at(-1), { kind: 'file', type: 'text/plain', data: Buffer.from('hello desk').toString('base64'), name: 'notes.txt', source: { app: 'Files' } });
  assert.equal(await panel.evaluate(() => typeof window.openagora.desk.pathForFile), 'undefined');
  const byPath = await panel.evaluate(() => window.openagora.desk.addFiles(['/etc/hostname']).then(() => 'added', (e) => e.message));
  assert.match(byPath, /invalid file/);
  assert.equal(deskAdds.length, 2);

  // Escape closes the panel; the shortcut brings it back. A never-captured
  // app is said so, and nothing is captured.
  await panel.keyboard.press('Escape');
  await waitFor(async () => !(await panelVisible(app)), 'the panel to close');
  verdict = 'never';
  await press(app);
  await panel.getByText('OpenAgora never captures Web notes.').waitFor();
  assert.equal(captureCalls.length, 1);

  // Where OpenAgora can't see which window is in front, the panel offers
  // the system's window picker only where the engine says there is one.
  let pick = false;
  daemon.handle('shortcut/pressed', () => ({ target: null, verdict: 'unknown', rule: null, unknown: 'the window list failed', pick, item: null, error: null }));
  for (const offered of [false, true]) {
    pick = offered;
    await panel.keyboard.press('Escape');
    await waitFor(async () => !(await panelVisible(app)), 'the panel to close');
    await press(app);
    await panel.getByText(/Can’t tell which window you’re in \(the window list failed\)/).waitFor();
    // The panel says the same with or without a picker, and can show the
    // last press for a moment: wait for this press's answer.
    const pickButton = panel.getByRole('button', { name: 'Pick a window' });
    await waitFor(async () => (await pickButton.count()) === (offered ? 1 : 0), `the pick button ${offered ? 'to show' : 'to be absent'}`);
  }
  await panel.getByRole('button', { name: 'Pick a window' }).click();
  await waitFor(() => captureCalls.length === 2, 'the pick to reach the engine');
  assert.deepEqual(captureCalls.at(-1), { pick: true });
  daemon.handle('shortcut/pressed', pressed);

  // No page in OpenAgora (an app's tab, Sunday's chat) gets the screen:
  // only OpenAgora's own capture page does, for the engine.
  for (const partition of ['persist:app-notes', 'persist:sunday']) {
    const media = await app.evaluate(async ({ webContents, session, BrowserWindow }, partition) => {
      const ses = session.fromPartition(partition);
      const view = webContents.getAllWebContents().find((c) => !c.isDestroyed() && c.session === ses);
      if (!view) return 'no view';
      const id = BrowserWindow.getAllWindows().find((w) => w.webContents.getURL().endsWith('index.html')).getMediaSourceId();
      const desktop = { audio: false, video: { mandatory: { chromeMediaSource: 'desktop', chromeMediaSourceId: id } } };
      return view.executeJavaScript(`navigator.mediaDevices.getUserMedia(${JSON.stringify(desktop)}).then((s) => { s.getTracks().forEach((t) => t.stop()); return 'granted'; }, (e) => e.name)`, true);
    }, partition);
    assert.equal(media, 'NotAllowedError', partition);
  }

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

  // Settings → Privacy: the apps Sunday may see and the never-capture list.
  await page.bringToFront();
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Settings');
  await page.getByRole('tab', { name: 'Privacy' }).click();
  assert.match(await page.getByRole('listitem').filter({ hasText: 'Built in' }).innerText(), /1Password, Private browser windows/);
  // Allowed apps can be taken back.
  await page.getByRole('listitem').filter({ hasText: 'Firefox' }).getByRole('button', { name: 'Remove' }).click();
  await waitFor(async () => (await page.getByRole('listitem').filter({ hasText: 'Firefox' }).count()) === 0, 'Firefox to go');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['broker-revoke', 'firefox']);
  // The never-capture list takes the user's own apps and window titles.
  await page.getByLabel('App', { exact: true }).fill('Signal');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('listitem').filter({ hasText: 'Signal' }).getByRole('button', { name: 'Remove' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['never-add', { name: 'Signal', app: 'Signal' }]);
  assert.equal(await page.getByLabel('App', { exact: true }).inputValue(), '');
  await page.getByRole('listitem').filter({ hasText: 'Signal' }).getByRole('button', { name: 'Remove' }).click();
  await waitFor(async () => (await fakeCalls(app)).at(-1)?.[0] === 'never-remove', 'Signal to go');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['never-remove', 'Signal']);

  // Settings → General: Sunday's shortcut is recorded by pressing it. A lone
  // modifier, or a key without one, isn't a shortcut.
  const settingsFile = path.join(directory, 'desktop-settings.json');
  const settings = () => (fs.existsSync(settingsFile) ? JSON.parse(fs.readFileSync(settingsFile, 'utf8')) : null);
  await page.getByRole('tab', { name: 'General' }).click();
  const shortcut = page.getByRole('textbox', { name: 'Sunday’s shortcut' });
  const mac = process.platform === 'darwin';
  assert.equal(await shortcut.inputValue(), mac ? '⇧⌘Space' : 'Ctrl+Shift+Space');
  await shortcut.click();
  await page.keyboard.press('Shift');
  await page.keyboard.press('K');
  assert.equal(settings(), null);
  await page.keyboard.press('ControlOrMeta+Alt+K');
  await waitFor(() => settings()?.shortcut === 'CommandOrControl+Alt+K', 'the new shortcut to be saved');
  assert.deepEqual(settings(), { version: 1, shortcut: 'CommandOrControl+Alt+K', keepRunning: true });
  await waitFor(async () => (await shortcut.inputValue()) === (mac ? '⌥⌘K' : 'Ctrl+Alt+K'), 'the new shortcut to show');
  assert.equal(await app.evaluate(() => globalThis.openagoraTest.shortcutRegistered()), true);
  // Keeping OpenAgora in the tray is on by default, and can be turned off.
  const keep = page.getByRole('checkbox', { name: /^Keep running in the/ });
  assert.equal(await keep.isChecked(), true);
  await keep.click();
  await waitFor(() => settings().keepRunning === false, 'keep running to be off');
  await waitFor(async () => !(await keep.isDisabled()), 'the setting to be saved');
  await keep.click();
  await waitFor(() => settings().keepRunning === true, 'keep running to be on');
  // Start with a hidden panel; on macOS closing the main window can focus
  // a visible panel, where the shortcut correctly toggles it closed.
  await panel.keyboard.press("Escape");
  await waitFor(async () => !(await panelVisible(app)), "the panel to hide");

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

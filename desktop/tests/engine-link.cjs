// OpenAgora's window and the engine service: connecting Sunday to OpenAgora
// (after showing exactly what changes), the engine's confirmations shown in
// the window, agents opening apps in tabs, and handoffs through the clipboard
// or an app's own URL scheme.
// The engine side is a stand-in; Rust tests cover it.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
let daemon;
let dashboard;
const directory = tempProfile('openagora-engine-link-');
/** A 1×1 PNG. */
const PIXEL = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

async function run() {
  dashboard = await fakeDashboard();
  daemon = await fakeDaemon();
  // The window reads the catalog itself before opening an app by its URL scheme.
  const engine = path.join(directory, 'fake-engine');
  fs.copyFileSync(path.join(__dirname, 'fixtures', 'fake-engine-catalog.cjs'), engine);
  fs.chmodSync(engine, 0o755);
  process.env.OPENAGORA_ENGINE = engine;
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  const notes = {
    id: 'notes', name: 'Notes', summary: 'A notes app', category: 'productivity',
    homepage: 'https://example.com', source: 'https://github.com/example/notes', license: 'MIT',
    platforms: ['macos', 'linux', 'windows'], available: true, role: 'app', profile: null, ui: 'web',
    steps: [], permissions: [], installed: true, adopted: false, present: true, status: 'running',
    pid: 42, url: dashboard.url, home_url: dashboard.url,
  };
  session = await launch({
    directory, daemon,
    fake: freshFakeState({ present: true, installed: true, clean: true, setup: 'ready', dashboardUrl: dashboard.url, extraApps: [notes] }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');

  // Before Sunday uses OpenAgora, the window shows exactly what changes in
  // Sunday's profile, and asks.
  await page.getByRole('button', { name: 'Start Sunday' }).click();
  await page.getByText('Let Sunday use OpenAgora?').waitFor();
  await page.getByRole('button', { name: 'Connect…' }).click();
  const consent = page.getByRole('dialog', { name: 'Let Sunday use OpenAgora?' });
  const changes = await consent.innerText();
  assert.match(changes, /hermes -p sunday config set mcp_servers\.openagora/);
  assert.match(changes, /hermes -p sunday config set hooks_auto_accept true/);
  await consent.getByRole('button', { name: 'Cancel' }).click();
  assert.deepEqual(await fakeCalls(app), [['start', 'hermes']]);
  await page.getByRole('button', { name: 'Connect…' }).click();
  await consent.getByRole('button', { name: 'Connect', exact: true }).click();
  await waitFor(async () => (await page.getByRole('button', { name: 'Connect…' }).count()) === 0, 'the question to go once Sunday uses OpenAgora');
  assert.deepEqual(await fakeCalls(app), [['start', 'hermes'], ['sunday-connect']]);

  // The engine asks the user in the window when the agent can't ask in its chat.
  daemon.send({ id: 'ask-1', method: 'confirm/ask', params: { client: 'sunday', title: 'Install Notes?', message: 'Install Notes? A notes app\nOn this computer OpenAgora will:\n  run: install-notes' } });
  const dialog = page.getByRole('alertdialog', { name: 'Install Notes?' });
  await dialog.waitFor();
  assert.equal(await dialog.locator('.modal-label').innerText(), 'Sunday');
  assert.match(await dialog.innerText(), /run: install-notes/);
  // "Don't allow" has the focus: Enter alone never allows.
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), 'Don’t allow');
  await dialog.getByRole('button', { name: 'Allow', exact: true }).click();
  assert.deepEqual((await daemon.reply('ask-1')).result, { allowed: true });
  await dialog.waitFor({ state: 'detached' });

  // Escape is a no; two questions are answered one at a time.
  daemon.send({ id: 'ask-2', method: 'confirm/ask', params: { client: 'sunday', title: 'Remove Notes?', message: 'Stop Notes and remove it?' } });
  daemon.send({ id: 'ask-3', method: 'confirm/ask', params: { client: 'sunday', title: 'Update Notes?', message: 'Update Notes?' } });
  await page.getByRole('alertdialog', { name: 'Remove Notes?' }).waitFor();
  assert.match(await page.getByRole('alertdialog').innerText(), /1 more waiting/);
  await page.keyboard.press('Escape');
  assert.deepEqual((await daemon.reply('ask-2')).result, { allowed: false });
  await page.getByRole('alertdialog', { name: 'Update Notes?' }).getByRole('button', { name: 'Don’t allow' }).click();
  assert.deepEqual((await daemon.reply('ask-3')).result, { allowed: false });

  // An agent asking to show an app opens its tab.
  const loaded = dashboard.requests.length;
  daemon.send({ method: 'app/open', params: { id: 'notes' } });
  await page.getByRole('button', { name: 'Notes', exact: true }).waitFor();
  await waitFor(() => dashboard.requests.length > loaded, 'the app tab to load');

  // Sunday hands things over on the clipboard: text, or a PNG or JPEG.
  daemon.send({ id: 'clip-1', method: 'clipboard/write', params: { kind: 'text', text: 'TODO: ship it' } });
  assert.deepEqual((await daemon.reply('clip-1')).result, { written: true });
  assert.equal(await app.evaluate(({ clipboard }) => clipboard.readText()), 'TODO: ship it');
  daemon.send({ id: 'clip-2', method: 'clipboard/write', params: { kind: 'image', type: 'image/png', data: PIXEL } });
  assert.deepEqual((await daemon.reply('clip-2')).result, { written: true });
  const pasted = await app.evaluate(async ({ clipboard, nativeImage }) => {
    for (const item of await clipboard.read()) {
      if (!item.types.includes('image/png')) continue;
      const blob = await item.getType('image/png');
      return nativeImage.createFromBuffer(Buffer.from(await blob.arrayBuffer())).getSize();
    }
    return null;
  });
  assert.deepEqual(pasted, { width: 1, height: 1 });
  daemon.send({ id: 'clip-3', method: 'clipboard/write', params: { kind: 'image', type: 'image/svg+xml', data: 'PHN2Zz4=' } });
  assert.match((await daemon.reply('clip-3')).error.message, /PNG or JPEG/);

  // An app that opens files by its own URL scheme: only a scheme an installed
  // app's listing names, never a web page, file or script.
  await app.evaluate(({ shell }) => {
    globalThis.opened = [];
    shell.openExternal = async (url) => { globalThis.opened.push(url); };
  });
  daemon.send({ id: 'url-1', method: 'url/open', params: { url: 'sketch://open?file=%2Ftmp%2Fa.png', scheme: 'sketch' } });
  assert.deepEqual((await daemon.reply('url-1')).result, { opened: true });
  for (const [id, url] of [['url-2', 'https://example.com/'], ['url-3', 'javascript:alert(1)'], ['url-4', 'file:///etc/passwd'], ['url-5', 'paint://open?file=x'], ['url-6', 'other://x']]) {
    daemon.send({ id, method: 'url/open', params: { url } });
    assert.ok((await daemon.reply(id)).error, `${url} was opened`);
  }
  assert.deepEqual(await app.evaluate(() => globalThis.opened), ['sketch://open?file=%2Ftmp%2Fa.png']);

  // Settings shows the connection and can undo it.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Settings');
  await page.getByRole('tab', { name: 'Sunday' }).click();
  const connected = page.getByRole('checkbox', { name: 'Let Sunday use OpenAgora' });
  assert.equal(await connected.isChecked(), true);
  await connected.click();
  await waitFor(async () => !(await connected.isChecked()), 'Sunday to stop using OpenAgora');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['sunday-disconnect']);
  assert.deepEqual(session.errors, []);
  console.log('Engine link UI passed: connect consent with the exact changes, confirmation allow/deny/escape/queue, app/open, clipboard, own URL schemes, disconnect.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

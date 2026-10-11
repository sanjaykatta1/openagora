// Popping an app out of its tab into a window of its own (acceptance test 13,
// the window's side): the app's page moves without reloading, OpenAgora's
// window tells the engine which window shows which app and which has focus,
// the popped-out page can be captured and its text read, a dialog of
// OpenAgora's is never hidden under an app's page, and closing the window
// brings the app back to its tab. Popped-out apps outlive the main window.
// The engine is a stand-in; engine/tests/capture.rs covers its side.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, tempProfile, waitFor } = require('./ui-helpers.cjs');

const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
let session;
let daemon;
let site;
const directory = tempProfile('openagora-popout-');

/** The Notes app's page, in OpenAgora's main process. */
const notesPage = (app, code) => app.evaluate(async ({ webContents, session }, code) => {
  const ses = session.fromPartition('persist:app-notes');
  const page = webContents.getAllWebContents().find((c) => !c.isDestroyed() && c.session === ses);
  return page ? page.executeJavaScript(code) : 'no page';
}, code);

const tabs = (app) => app.evaluate(() => globalThis.openagoraTest.tabs());
const reports = () => daemon.received.filter((m) => m.method === 'focus/openagora').map((m) => m.params);
const lastReport = () => reports().at(-1);
/** OpenAgora's windows of popped-out apps. */
const popoutWindows = (app) => app.evaluate(({ BaseWindow, BrowserWindow }) => {
  const browsers = new Set(BrowserWindow.getAllWindows().map((w) => w.id));
  return BaseWindow.getAllWindows().filter((w) => !browsers.has(w.id) && !w.isDestroyed()).map((w) => ({ id: w.id, title: w.getTitle(), source: w.getMediaSourceId() }));
});

async function run() {
  site = await fakeDashboard();
  daemon = await fakeDaemon();
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  const notes = {
    id: 'notes', name: 'Web notes', summary: 'A notes app', category: 'productivity',
    homepage: 'https://example.com', source: 'https://github.com/example/notes', license: 'MIT',
    platforms: ['macos', 'linux', 'windows'], available: true, role: 'app', profile: null, ui: 'web',
    steps: [], permissions: [], installed: true, adopted: false, present: true, status: 'running',
    pid: 42, url: `${site.url}notes`, home_url: `${site.url}notes`,
  };
  session = await launch({
    directory, daemon,
    fake: freshFakeState({
      present: true, installed: true, clean: true, setup: 'ready', running: true, pid: 7, connected: true,
      dashboardUrl: site.url, extraApps: [notes],
    }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');

  // The Notes tab: its page belongs to OpenAgora's main process, drawn over
  // the main window where the tab shows. (Sunday's home shows first.)
  await page.locator('webview').first().waitFor({ state: 'attached' });
  daemon.send({ method: 'app/open', params: { id: 'notes' } });
  await page.getByRole('button', { name: 'Web notes', exact: true }).waitFor();
  await waitFor(async () => (await tabs(app)).some((t) => t.id === 'notes' && t.visible && t.inMain), 'the Notes page in the main window');
  await waitFor(() => lastReport()?.tab === 'notes', 'the window to report the Notes tab');
  await waitFor(async () => (await notesPage(app, 'document.readyState')) === 'complete', 'the Notes page to load');
  const [shown] = await tabs(app);
  assert.ok(shown.bounds.width > 300 && shown.bounds.height > 200, JSON.stringify(shown.bounds));
  assert.ok(shown.bounds.y > 0, 'below the tab’s own bar');
  assert.ok(site.requests.includes('/notes'));
  await notesPage(app, 'window.marker = 42; document.body.dataset.typed = "draft"');
  const loads = site.requests.filter((r) => r === '/notes').length;

  // A dialog of OpenAgora's would be hidden under the app's page: the page
  // steps aside while it is open.
  daemon.send({ id: 'ask-1', method: 'confirm/ask', params: { id: 'q1', client: 'sunday', title: 'Install Web paint?', message: 'Sunday asks to install Web paint.' } });
  const dialog = page.getByRole('alertdialog', { name: 'Install Web paint?' });
  await dialog.waitFor();
  await waitFor(async () => !(await tabs(app))[0].visible, 'the page to step aside for the dialog');
  await dialog.getByRole('button', { name: 'Don’t allow' }).click();
  await daemon.reply('ask-1');
  await waitFor(async () => (await tabs(app))[0].visible, 'the page to come back after the dialog');

  // Pop out: the same page, in a window of its own, not reloaded.
  await page.getByRole('button', { name: 'Pop out' }).click();
  await page.getByText('Web notes is open in its own window.').waitFor();
  let windows = [];
  await waitFor(async () => (windows = await popoutWindows(app)).length === 1, 'the popped-out window');
  assert.equal(windows[0].title, 'Web notes');
  assert.equal(await notesPage(app, 'window.marker'), 42, 'the page was reloaded');
  assert.equal(await notesPage(app, 'document.body.dataset.typed'), 'draft');
  assert.equal(site.requests.filter((r) => r === '/notes').length, loads);
  const [popped] = await tabs(app);
  assert.equal(popped.popped, true);
  assert.equal(popped.inMain, false);
  // The engine hears which window shows Notes, and that the main window
  // shows no app now.
  await waitFor(() => lastReport()?.popouts?.length === 1, 'the window to report the popped-out app');
  assert.deepEqual(lastReport().popouts, [{ window: windows[0].source, app: 'notes' }]);
  assert.equal(lastReport().tab, null);
  assert.match(windows[0].source, /^window:\d+:\d+$/);
  // When it has focus, the engine hears that too (Wayland doesn't tell it).
  await app.evaluate(({ BaseWindow }, id) => BaseWindow.fromId(id).focus(), windows[0].id);
  await waitFor(() => reports().some((r) => r.focused?.window === 'popout' && r.focused.app === 'notes'), 'the window to report the popped-out app’s focus');

  // The engine can capture it there, as it captures a tab: the app's page.
  const capture = await daemon.ask('capture/window', { target: { kind: 'tab', app: 'notes' }, max: 1920 });
  assert.ok(capture.result, JSON.stringify(capture.error));
  assert.ok(Buffer.from(capture.result.data, 'base64').subarray(0, 8).equals(PNG));
  assert.ok(capture.result.width > 300);
  // And read its text, from the app's page: no OS permission needed.
  const read = await daemon.ask('capture/text', { target: { kind: 'tab', app: 'notes' } });
  assert.ok(read.result, JSON.stringify(read.error));
  assert.match(read.result.text, /Fake Hermes dashboard\s+\/notes/);
  assert.equal(read.result.title, 'Hermes');
  // Other windows' text the engine reads itself, through the OS.
  const refused = await daemon.ask('capture/text', { target: { kind: 'window', id: 7 } });
  assert.match(refused.error.message, /its own tabs only/);

  // Bring it back: the same page, in its tab again.
  await page.bringToFront();
  await page.getByRole('button', { name: 'Bring it back here' }).click();
  await waitFor(async () => (await popoutWindows(app)).length === 0, 'the popped-out window to close');
  await waitFor(async () => { const [t] = await tabs(app); return t.visible && t.inMain && !t.popped; }, 'the page back in its tab');
  assert.equal(await notesPage(app, 'window.marker'), 42);
  await waitFor(() => lastReport()?.tab === 'notes' && lastReport().popouts.length === 0, 'the window to report Notes back in its tab');

  // Closing the popped-out window brings it back too.
  await page.getByRole('button', { name: 'Pop out' }).click();
  await waitFor(async () => (windows = await popoutWindows(app)).length === 1, 'the popped-out window again');
  await app.evaluate(({ BaseWindow }, id) => BaseWindow.fromId(id).close(), windows[0].id);
  await waitFor(async () => { const [t] = await tabs(app); return t && t.visible && t.inMain; }, 'the page back after closing its window');
  await page.getByRole('button', { name: 'Pop out' }).waitFor();
  assert.equal(await notesPage(app, 'window.marker'), 42);

  // Popped-out apps outlive the main window, and a main window that opens
  // again lists them.
  await page.getByRole('button', { name: 'Pop out' }).click();
  await waitFor(async () => (await popoutWindows(app)).length === 1, 'the popped-out window before the main window closes');
  await app.evaluate(() => globalThis.openagoraTest.main()?.close());
  await waitFor(async () => (await app.evaluate(() => !globalThis.openagoraTest.main())), 'the main window to close');
  assert.equal((await popoutWindows(app)).length, 1);
  assert.equal(await notesPage(app, 'window.marker'), 42);
  await waitFor(() => lastReport()?.main === null && lastReport().popouts.length === 1, 'the window to report only the popped-out app');
  await app.evaluate(() => globalThis.openagoraTest.showMain());
  let main;
  await waitFor(() => (main = app.windows().find((w) => w.url().endsWith('index.html') && !w.isClosed())), 'the main window again');
  await main.getByRole('button', { name: 'Web notes', exact: true }).click();
  await main.getByText('Web notes is open in its own window.').waitFor();
  await main.getByRole('button', { name: 'Bring it back here' }).click();
  await waitFor(async () => { const [t] = await tabs(app); return t && t.visible && t.inMain; }, 'the page in the new main window');
  assert.equal(await notesPage(app, 'window.marker'), 42);

  // Closing the tab closes its page, popped out or not.
  await main.getByRole('button', { name: 'Pop out' }).click();
  await waitFor(async () => (await popoutWindows(app)).length === 1, 'the popped-out window before the tab closes');
  await main.getByRole('button', { name: 'Close Web notes tab' }).click();
  await waitFor(async () => (await popoutWindows(app)).length === 0 && (await tabs(app)).length === 0, 'the page to close with its tab');
  assert.deepEqual(session.errors, []);
  console.log('Pop-out UI passed: same page without reloading, the engine told which window shows which app and which has focus, capture, dialogs never hidden, back to the tab, outlives the main window, closes with its tab.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (site) site.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

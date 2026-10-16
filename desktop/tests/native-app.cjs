// Native apps use their own window; missing files offer a reinstall.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

const directory = tempProfile('openagora-native-');
let session;
async function run() {
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: null }));
  const handy = {
    id: 'handy', name: 'Handy', summary: 'Offline dictation', category: 'utilities',
    homepage: 'https://handy.computer', source: 'https://github.com/cjpais/Handy', license: 'MIT',
    platforms: ['macos', 'linux'], available: true, role: 'app', profile: null,
    ui: 'background', can_open_window: true, steps: [], permissions: [],
    setup_notes: 'Follow Handy’s own setup for dictation.',
    installed: true, adopted: false, present: true, status: 'stopped', pid: null, url: null,
  };
  session = await launch({ directory, fake: freshFakeState({ extraApps: [handy] }) });
  const { app, page } = session;
  await app.evaluate(({ ipcMain }) => {
    ipcMain.removeHandler('open');
    ipcMain.handle('open', (_event, id) => {
      globalThis.fake.calls.push(['open', id]);
      const item = globalThis.fake.extraApps.find((a) => a.id === id);
      item.status = 'running'; item.pid = 1234;
      return { ok: true, stdout: 'Asked Handy to show its window.', stderr: '' };
    });
    ipcMain.removeHandler('install');
    ipcMain.handle('install', (_event, id) => {
      globalThis.fake.calls.push(['install', id]);
      const item = globalThis.fake.extraApps.find((a) => a.id === id);
      item.installed = true; item.present = true; item.status = 'stopped';
      return { ok: true, stdout: '', stderr: '' };
    });
  });
  await page.getByRole('button', { name: /Library/ }).click();
  await heading(page, 'Library');
  const row = page.locator('.row').filter({ hasText: 'Handy' });
  await row.getByRole('button', { name: 'Open', exact: true }).click();
  await waitFor(async () => (await fakeCalls(app)).length === 1, 'native app open');
  await row.getByRole('button', { name: 'Stop', exact: true }).waitFor();
  await row.getByRole('button', { name: 'Open', exact: true }).click();
  await waitFor(async () => (await fakeCalls(app)).length === 2, 'native app reopen');
  assert.deepEqual(await fakeCalls(app), [['open', 'handy'], ['open', 'handy']]);
  assert.equal(await page.locator('.webview-bar').count(), 0);

  await app.evaluate(() => { const a = globalThis.fake.extraApps[0]; a.present = false; a.status = 'stopped'; });
  await page.reload();
  await page.getByRole('button', { name: /Library/ }).click();
  await page.getByText('App files missing · open its page to reinstall').waitFor();
  await row.locator('.row-main').click();
  await heading(page, 'Handy');
  await page.getByText('Follow Handy’s own setup for dictation.').waitFor();
  await page.getByRole('button', { name: 'Reinstall missing app' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Install', exact: true }).click();
  await page.getByRole('button', { name: 'Open', exact: true }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['install', 'handy']);
  assert.deepEqual(session.errors, []);
  console.log('Native app UI passed: open, reopen, no blank web tab, and reinstall missing files.');
}
run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  fs.rmSync(directory, { recursive: true, force: true });
});

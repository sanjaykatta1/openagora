// Background mode (acceptance test 9, the window's side): off by default;
// turned on only after the user reads what it keeps; pause, resume, how long
// captures are kept and clear-all go to the engine; the sidebar and the tray
// show while it captures, as the engine tells the window. The engine is a
// stand-in; engine/tests/background.rs covers its side.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
let daemon;
let dashboard;
const directory = tempProfile('openagora-background-');

const tray = (app) => app.evaluate(() => globalThis.openagoraTest.background());

async function run() {
  dashboard = await fakeDashboard();
  daemon = await fakeDaemon();
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  session = await launch({
    directory, daemon,
    fake: freshFakeState({ present: true, installed: true, clean: true, setup: 'ready', running: true, pid: 7, connected: true, dashboardUrl: dashboard.url }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Background mode');
  const rows = page.getByRole('list', { name: 'Background mode' });
  const indicator = page.locator('.background-indicator');

  // Off by default, and nothing says it captures.
  await rows.getByText('Off', { exact: true }).waitFor();
  assert.equal(await indicator.count(), 0);
  assert.doesNotMatch((await tray(app)).tooltip, /Background capture/);

  // Turned on only after the user reads what it keeps.
  await rows.getByRole('button', { name: 'Turn on…' }).click();
  const ask = page.getByRole('dialog', { name: 'Turn on background mode?' });
  const told = await ask.innerText();
  assert.match(told, /apps you always allowed Sunday to see/);
  assert.match(told, /never-capture list/);
  assert.match(told, /for 7 days/);
  assert.match(told, /sent to the model provider/);
  await ask.getByRole('button', { name: 'Cancel' }).click();
  assert.ok(!(await fakeCalls(app)).some((c) => c[0] === 'background-turn'));
  await rows.getByRole('button', { name: 'Turn on…' }).click();
  await ask.getByRole('button', { name: 'Turn on' }).click();
  await page.getByRole('status').filter({ hasText: 'Background mode is on.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-turn', true]);
  await rows.getByText('Background capture is on').waitFor();
  await indicator.filter({ hasText: 'Background capture is on' }).waitFor();

  // The engine tells the window, which shows it in the tray too.
  daemon.send({ method: 'background/state', params: { on: true, paused: false, paused_until: null, keep_days: 7 } });
  await waitFor(async () => /Background capture is on/.test((await tray(app)).tooltip), 'the tray to show background capture');
  assert.deepEqual((await tray(app)).state, { on: true, paused: false, pausedUntil: null, keepDays: 7 });

  // Pause for an hour, then resume.
  await rows.getByRole('button', { name: 'Pause for an hour' }).click();
  await page.getByRole('status').filter({ hasText: 'paused for an hour' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-pause', 60]);
  await rows.getByText(/Background capture is paused until \d/).waitFor();
  await waitFor(async () => (await indicator.getAttribute('class'))?.includes('paused'), 'the sidebar to show the pause');
  daemon.send({ method: 'background/state', params: { on: true, paused: true, paused_until: Math.floor(Date.now() / 1000) + 3600, keep_days: 7 } });
  await waitFor(async () => /paused until/.test((await tray(app)).tooltip), 'the tray to show the pause');
  await rows.getByRole('button', { name: 'Resume' }).click();
  await page.getByRole('status').filter({ hasText: 'Background capture is on again.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-resume']);
  await rows.getByRole('button', { name: 'Pause', exact: true }).click();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-pause', null]);
  await rows.getByText('Background capture is paused', { exact: true }).waitFor();
  await rows.getByRole('button', { name: 'Resume' }).click();

  // How long captures are kept.
  await app.evaluate(() => { globalThis.fake.background = { ...globalThis.fake.background, captures: 12, bytes: 3 * 1024 * 1024 }; });
  await page.getByRole('combobox', { name: 'Keep captures for' }).selectOption('14');
  await page.getByRole('status').filter({ hasText: 'keeps captures for 14 days' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-keep', 14]);
  await rows.getByText('Kept: 12 captures, 3.0 MB, for 14 days.').waitFor();

  // Clear all, after a question.
  await rows.getByRole('button', { name: 'Clear all…' }).click();
  const clear = page.getByRole('dialog', { name: 'Delete everything background mode kept?' });
  assert.match(await clear.innerText(), /12 captures, 3\.0 MB/);
  await clear.getByRole('button', { name: 'Delete' }).click();
  await page.getByRole('status').filter({ hasText: 'Everything background mode kept is deleted.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-clear']);
  assert.equal(await rows.getByRole('button', { name: 'Clear all…' }).isDisabled(), true);

  // Off: the sidebar and the tray stop showing it.
  await rows.getByRole('button', { name: 'Turn off' }).click();
  await page.getByRole('status').filter({ hasText: 'Background mode is off.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['background-turn', false]);
  await waitFor(async () => (await indicator.count()) === 0, 'the sidebar indicator to go');
  daemon.send({ method: 'background/state', params: { on: false, paused: false, paused_until: null, keep_days: 14 } });
  await waitFor(async () => !/Background capture/.test((await tray(app)).tooltip), 'the tray to stop showing background capture');
  assert.deepEqual(session.errors, []);
  console.log('Background mode UI passed: off by default, on after reading what it keeps, pause and resume, how long it keeps, clear all, off; sidebar and tray show it.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

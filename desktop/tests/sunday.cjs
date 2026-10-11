// Sunday in the desktop app, end to end against a fake engine and a fake
// Hermes dashboard: install → Sunday's own profile → Hermes's own setup;
// an existing Hermes user; Settings → Sunday; removing Sunday.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
const dirs = [];

async function freshUser(dashboard) {
  const directory = tempProfile('openagora-sunday-');
  dirs.push(directory);
  session = await launch({ directory, fake: freshFakeState({ dashboardUrl: dashboard.url }) });
  const { app, page } = session;

  // First launch: choose Sunday, which becomes the home screen.
  await heading(page, 'Choose your personal agent');
  await page.getByRole('radio', { name: /^Sunday/ }).check();
  await page.getByRole('button', { name: 'Continue with Sunday' }).click();
  await heading(page, 'Install Hermes to meet Sunday');
  assert.match(await page.locator('body').innerText(), /powered by Hermes/);

  // Installing Hermes reviews the official installer first, then sets up
  // Sunday's own profile and opens Hermes's own model setup.
  await page.getByRole('button', { name: 'Install Hermes' }).click();
  const dialog = page.getByRole('dialog', { name: 'Install Hermes?' });
  await dialog.waitFor();
  assert.match(await dialog.innerText(), /creates Hermes’s “sunday” profile for Sunday/);
  await dialog.getByRole('button', { name: 'Install', exact: true }).click();
  await waitFor(() => dashboard.requests.includes('/models?profile=sunday'), 'the dashboard to open Hermes’s model setup');
  assert.deepEqual(await fakeCalls(app), [['install', 'hermes'], ['sunday-setup', 'default'], ['start', 'hermes']]);
  const bar = page.locator('.webview-bar');
  assert.match(await bar.innerText(), /Sunday/);
  assert.match(await bar.innerText(), /powered by Hermes/);

  // Settings → Sunday opens Hermes's own pages for Sunday's profile.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Settings');
  assert.match(await page.locator('body').innerText(), /Messaging gateway stopped/);
  for (const name of ['Models', 'Channels', 'Pairing', 'Skills', 'Plugins', 'MCP', 'Cron', 'Config', 'Keys', 'Sessions', 'Logs']) {
    await page.getByRole('button', { name: new RegExp(`^${name}`) }).waitFor();
  }
  await page.getByRole('button', { name: /^Channels/ }).click();
  await waitFor(() => dashboard.requests.includes('/channels?profile=sunday'), 'the Channels page for Sunday');
  assert.equal(await page.locator('.webview-bar').isVisible(), true);

  // Hermes's own terminal setup for Sunday names Sunday's profile.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('button', { name: 'Run Sunday’s setup in Terminal' }).click();
  await page.getByRole('status').filter({ hasText: 'Opened Hermes’s own setup' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['launch-agent', 'hermes', 'setup']);

  // Removing Sunday asks first, then removes only Sunday's profile.
  await page.getByRole('button', { name: 'Remove Sunday…' }).click();
  await page.getByRole('button', { name: 'Remove Sunday', exact: true }).click();
  await waitFor(async () => (await fakeCalls(app)).some((c) => c[0] === 'uninstall'), 'the uninstall');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['uninstall', 'hermes']);
  await page.getByRole('status').filter({ hasText: 'Sunday removed. Hermes is still installed.' }).waitFor();

  // Hermes is still here, so Sunday now asks how to start again.
  await page.getByRole('button', { name: /^Sunday/ }).click();
  await heading(page, 'You already use Hermes');
  assert.deepEqual(session.errors, []);
  await app.close();

  // After a restart Sunday is still the home screen.
  session = await launch({ directory, fake: freshFakeState({ present: true, setup: 'ready', installed: true, running: true, pid: 7, dashboardUrl: dashboard.url }) });
  await session.page.locator('.webview-bar').waitFor();
  await waitFor(() => dashboard.requests.filter((r) => r === '/?profile=sunday').length > 0, 'the dashboard home');
  await session.app.close();
}

async function existingHermesUser(dashboard) {
  const directory = tempProfile('openagora-sunday-existing-');
  dirs.push(directory);
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  session = await launch({ directory, fake: freshFakeState({ present: true, version: '0.20.2', setup: 'missing', dashboardUrl: dashboard.url }) });
  const { app, page } = session;

  // An older Hermes is updated with its own command first.
  await heading(page, 'Update Hermes for Sunday');
  assert.match(await page.locator('body').innerText(), /0\.21\.3 or later, and this computer has 0\.20\.2/);
  await page.getByRole('button', { name: 'Update Hermes' }).click();

  // Then the user decides how Sunday starts; nothing is reinstalled.
  await heading(page, 'You already use Hermes');
  assert.equal(await page.getByRole('button', { name: 'Set up Sunday' }).isDisabled(), true);
  await page.getByRole('radio', { name: /Bring my setup/ }).check();
  await page.getByRole('checkbox', { name: /Also copy my messaging bots/ }).check();
  assert.match(await page.locator('body').innerText(), /A messaging bot can answer for only one profile at a time/);
  await page.getByRole('button', { name: 'Set up Sunday' }).click();
  await waitFor(() => dashboard.requests.length > 0, 'Sunday’s dashboard');
  const calls = await fakeCalls(app);
  assert.deepEqual(calls, [['update-app', 'hermes'], ['sunday-setup', 'clone-channels'], ['start', 'hermes']]);
  assert.equal(calls.some((c) => c[0] === 'install'), false);
  // A copied setup already has a model, so the dashboard opens at its home.
  assert.equal(dashboard.requests.at(-1), '/?profile=sunday');
  assert.deepEqual(session.errors, []);
  await app.close();

  // Starting fresh is the other choice.
  session = await launch({ directory, fake: freshFakeState({ present: true, setup: 'missing', dashboardUrl: dashboard.url }) });
  await heading(session.page, 'You already use Hermes');
  await session.page.getByRole('radio', { name: /Start fresh/ }).check();
  await session.page.getByRole('button', { name: 'Set up Sunday' }).click();
  await waitFor(async () => (await fakeCalls(session.app)).length >= 2, 'setup');
  assert.deepEqual((await fakeCalls(session.app))[0], ['sunday-setup', 'fresh']);
  await session.app.close();
}

async function run() {
  const dashboard = await fakeDashboard();
  try {
    await freshUser(dashboard);
    dashboard.requests.length = 0;
    await existingHermesUser(dashboard);
  } finally {
    dashboard.close();
  }
  console.log('Sunday UI passed: install, own profile, Hermes setup, settings pages, terminal setup, removal, restart, update, bring my setup, start fresh.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});

// Sunday in the desktop app, end to end against a fake engine and a fake
// Hermes dashboard: install → Sunday's own profile → Hermes's own setup;
// an existing Hermes user; Settings → Sunday; removing Sunday.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
const dirs = [];

/** The bar above Sunday's dashboard, once it runs. */
const dashboardBar = (page) => page.locator('.bar').filter({ hasText: 'Open in browser' }).filter({ hasText: 'Sunday' });

async function freshUser(dashboard) {
  const directory = tempProfile('openagora-sunday-');
  dirs.push(directory);
  session = await launch({ directory, fake: freshFakeState({ dashboardUrl: dashboard.url }) });
  const { app, page } = session;
  const bar = dashboardBar(page);

  // First launch: use Sunday, which becomes the home screen. Credit where
  // it's due: Sunday is Hermes Agent, by Nous Research.
  await heading(page, 'Sunday');
  assert.match(await page.locator('body').innerText(), /Hermes Agent by Nous Research/);
  await page.getByRole('button', { name: 'Use Sunday' }).click();

  // Installing Hermes shows its official source and steps first, then sets
  // up Sunday's own profile and opens Hermes's own model setup.
  await page.getByRole('button', { name: 'Install Hermes' }).click();
  const dialog = page.getByRole('dialog', { name: 'Install Hermes?' });
  await dialog.waitFor();
  assert.match(await dialog.innerText(), /github\.com\/NousResearch\/hermes-agent/);
  await dialog.getByRole('button', { name: 'Install', exact: true }).click();
  await waitFor(() => dashboard.requests.includes('/models?profile=sunday'), 'the dashboard to open Hermes’s model setup');
  assert.deepEqual(await fakeCalls(app), [['install', 'hermes'], ['sunday-setup', 'default'], ['start', 'hermes']]);
  assert.match(await bar.innerText(), /Sunday/);

  // Settings → Sunday opens Hermes's own pages for Sunday's profile, and
  // credits Hermes with its license.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Settings');
  await page.getByRole('tab', { name: 'Sunday' }).click();
  assert.match(await page.locator('body').innerText(), /Hermes Agent by Nous Research · MIT license/);
  assert.match(await page.getByRole('listitem').filter({ hasText: 'Messaging gateway' }).innerText(), /Stopped/);
  for (const name of ['Models', 'Channels', 'Pairing', 'Skills', 'Plugins', 'MCP', 'Cron', 'Config', 'Keys', 'Sessions', 'Logs', 'System']) {
    await page.getByRole('button', { name, exact: true }).waitFor();
  }
  await page.getByRole('button', { name: 'Channels', exact: true }).click();
  await waitFor(() => dashboard.requests.includes('/channels?profile=sunday'), 'the Channels page for Sunday');
  assert.equal(await bar.isVisible(), true);

  // Hermes's own terminal setup for Sunday names Sunday's profile. Settings
  // remembers its tab.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('button', { name: 'Run setup in Terminal' }).click();
  await waitFor(async () => (await fakeCalls(app)).at(-1)?.[0] === 'launch-agent', 'the terminal setup');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['launch-agent', 'hermes', 'setup']);

  // Removing Sunday asks first, then removes only Sunday's profile.
  await page.getByRole('button', { name: 'Remove Sunday…' }).click();
  const remove = page.getByRole('dialog', { name: 'Remove Sunday?' });
  assert.match(await remove.innerText(), /Deletes Sunday’s Hermes profile/);
  await remove.getByRole('button', { name: 'Remove', exact: true }).click();
  await waitFor(async () => (await fakeCalls(app)).some((c) => c[0] === 'uninstall'), 'the uninstall');
  assert.deepEqual((await fakeCalls(app)).at(-1), ['uninstall', 'hermes']);
  await page.getByRole('status').filter({ hasText: 'Sunday removed.' }).waitFor();

  // Hermes is still here, so Sunday now asks how to start again.
  await page.getByRole('button', { name: /^Sunday/ }).click();
  await page.getByRole('radio', { name: 'Start fresh' }).check();
  await page.getByRole('button', { name: 'Set up Sunday' }).click();
  await waitFor(async () => (await fakeCalls(app)).filter((c) => c[0] === 'start').length === 2, 'recreated Sunday to start');
  await bar.waitFor();
  assert.deepEqual((await fakeCalls(app)).slice(-2), [['sunday-setup', 'fresh'], ['start', 'hermes']]);
  assert.deepEqual(session.errors, []);
  await app.close();

  // After a restart Sunday is still the home screen.
  session = await launch({ directory, fake: freshFakeState({ present: true, setup: 'ready', installed: true, running: true, pid: 7, dashboardUrl: dashboard.url }) });
  await dashboardBar(session.page).waitFor();
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
  await page.getByRole('button', { name: 'Update Hermes' }).click();

  // Then the user decides how Sunday starts; nothing is reinstalled.
  await page.getByRole('radio', { name: 'Start fresh' }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Set up Sunday' }).isDisabled(), true);
  assert.equal(await page.getByRole('checkbox', { name: 'Include messaging bots' }).count(), 0);
  await page.getByRole('radio', { name: 'Bring my Hermes setup' }).check();
  await page.getByRole('checkbox', { name: 'Include messaging bots' }).check();
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
  await session.page.getByRole('radio', { name: 'Start fresh' }).check();
  await session.page.getByRole('button', { name: 'Set up Sunday' }).click();
  await waitFor(async () => (await fakeCalls(session.app)).length >= 2, 'setup');
  assert.deepEqual((await fakeCalls(session.app))[0], ['sunday-setup', 'fresh']);
  await session.app.close();
}

async function retryAndExternalRemoval(dashboard) {
  const directory = tempProfile('openagora-sunday-retry-');
  dirs.push(directory);
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  session = await launch({ directory, fake: freshFakeState({ present: true, installed: true, clean: true, setup: 'missing', setupFailures: 1, dashboardUrl: dashboard.url }) });
  const { app, page } = session;
  const error = page.getByRole('alert').filter({ hasText: 'Temporary profile creation failure' });
  await error.waitFor();
  assert.equal((await fakeCalls(app)).filter((c) => c[0] === 'sunday-setup').length, 1);
  await error.getByRole('button', { name: 'Try again' }).click();
  await dashboardBar(page).waitFor();
  assert.equal((await fakeCalls(app)).filter((c) => c[0] === 'sunday-setup').length, 2);
  assert.deepEqual((await fakeCalls(app)).at(-1), ['start', 'hermes']);
  // Losing a profile while the home screen stays mounted must trigger fresh setup again.
  await app.evaluate(() => { globalThis.fake.setup = 'missing'; globalThis.fake.running = false; });
  await waitFor(async () => (await fakeCalls(app)).filter((c) => c[0] === 'start').length === 2, 'external profile removal recovery', 30000);
  await dashboardBar(page).waitFor();
  assert.deepEqual(session.errors, []);
  await app.close();
}

async function run() {
  const dashboard = await fakeDashboard();
  try {
    await freshUser(dashboard);
    dashboard.requests.length = 0;
    await existingHermesUser(dashboard);
    await retryAndExternalRemoval(dashboard);
  } finally {
    dashboard.close();
  }
  console.log('Sunday UI passed: install, own profile, Hermes setup, settings pages, terminal setup, removal, restart, update, bring my setup, start fresh, retry.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});

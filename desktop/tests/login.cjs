// Opening OpenAgora at login (the issue's default: off, with a setting): the
// setting writes OpenAgora's own autostart entry (Linux; macOS and Windows
// keep login items themselves) and removes it again, and a start at login
// opens in the tray without the window until the user asks for it.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { _electron: electron } = require('playwright-core');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
let daemon;
let dashboard;
let hidden;
const directory = tempProfile('openagora-login-');
const config = tempProfile('openagora-login-config-');
// OpenAgora's autostart entry goes here, not into this computer's own.
process.env.XDG_CONFIG_HOME = config;
const entry = path.join(config, 'autostart', 'openagora.desktop');

async function run() {
  if (process.platform !== 'linux') {
    console.log('Login UI test skipped (the autostart entry is Linux\'s; macOS and Windows keep login items themselves).');
    return;
  }
  dashboard = await fakeDashboard();
  daemon = await fakeDaemon();
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  session = await launch({
    directory, daemon,
    fake: freshFakeState({ present: true, installed: true, clean: true, setup: 'ready', running: true, pid: 7, connected: true, dashboardUrl: dashboard.url }),
  });
  const { app, page } = session;
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'What Sunday can see');
  const box = page.getByRole('checkbox', { name: /Open OpenAgora when you log in/ });
  // Off by default.
  assert.equal(await box.isChecked(), false);
  assert.equal(fs.existsSync(entry), false);
  await box.check();
  await page.getByRole('status').filter({ hasText: 'OpenAgora opens when you log in.' }).waitFor();
  const written = fs.readFileSync(entry, 'utf8');
  const program = await app.evaluate(() => process.execPath);
  assert.match(written, /^\[Desktop Entry\]\nType=Application\n/);
  assert.ok(written.includes(`\nExec="${program}" --hidden\n`), written);
  assert.match(await page.locator('body').innerText(), /It opens in the tray, without its window/);
  await box.uncheck();
  await page.getByRole('status').filter({ hasText: 'OpenAgora no longer opens when you log in.' }).waitFor();
  assert.equal(fs.existsSync(entry), false);
  assert.deepEqual(session.errors, []);
  await app.close();
  session = null;

  // Started at login: only the tray, until the user opens the window.
  hidden = await electron.launch({
    args: ['.', '--hidden', `--user-data-dir=${directory}`, '--no-sandbox'],
    cwd: path.resolve(__dirname, '..'),
    env: { ...process.env, OPENAGORA_TEST: '1', OPENAGORA_DAEMON_ADDRESS: daemon.address },
  });
  await waitFor(() => hidden.evaluate(() => !!globalThis.openagoraTest?.tray()), 'the tray icon');
  await new Promise((r) => setTimeout(r, 1000));
  assert.equal(await hidden.evaluate(() => globalThis.openagoraTest.main()), null);
  assert.equal(hidden.windows().length, 0);
  await hidden.evaluate(() => globalThis.openagoraTest.showMain());
  await waitFor(() => hidden.windows().some((w) => w.url().endsWith('index.html')), 'the window, once asked for');
  console.log('Login UI passed: off by default, the autostart entry written and removed, a start at login in the tray only.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (hidden) await hidden.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
  fs.rmSync(config, { recursive: true, force: true });
});

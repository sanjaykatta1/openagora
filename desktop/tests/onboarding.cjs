// Runs the packaged renderer and real preferences IPC in an isolated Electron profile.
// No agent is installed or started by this test.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { _electron: electron } = require('playwright-core');

const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-onboarding-'));
const file = path.join(directory, 'personal-agent.json');
let app;
let page;
const errors = [];

async function launch() {
  app = await electron.launch({
    args: ['.', `--user-data-dir=${directory}`, ...(process.platform === 'linux' ? ['--no-sandbox'] : [])],
    cwd: path.resolve(__dirname, '..'),
  });
  assert.equal(fs.realpathSync(await app.evaluate(({ app }) => app.getPath('userData'))), fs.realpathSync(directory));
  page = await app.firstWindow();
  page.on('pageerror', (e) => errors.push(e.message));
  // Make catalog availability deterministic; leave the preference bridge real.
  await app.evaluate(({ ipcMain }) => {
    ipcMain.removeHandler('catalog');
    globalThis.fixtureListing = {
      id: 'hermes', name: 'Hermes', summary: 'An agent with memory', category: 'agents',
      homepage: 'https://hermes-agent.nousresearch.com', source: 'https://github.com/NousResearch/hermes-agent',
      license: 'MIT', platforms: ['macos', 'linux', 'windows'], available: true, role: 'agent',
      ui: 'web', steps: [], permissions: [], installed: false, status: 'not-installed',
      pid: null, url: null, home_url: null,
    };
    ipcMain.handle('catalog', () => [globalThis.fixtureListing]);
    ipcMain.removeHandler('install');
    ipcMain.handle('install', () => { globalThis.fixtureListing.installed = true; globalThis.fixtureListing.status = 'stopped'; return { ok: true, stdout: '', stderr: '' }; });
  });
  await page.reload();
}

async function heading(text) {
  await page.getByRole('heading', { name: text, exact: true }).waitFor();
}

async function choose(name) {
  await page.getByRole('radio', { name, exact: false }).check();
  await page.getByRole('button', { name: `Continue with ${name}`, exact: true }).click();
  await heading('Personal agent');
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, name.toLowerCase());
}

async function run() {
  await launch();
  await heading('Choose your personal agent');
  assert.equal(await page.getByRole('radio').count(), 7);
  assert.equal(await page.locator('input:checked').count(), 0);
  assert.equal(await page.getByRole('button', { name: 'Select an agent to continue' }).isDisabled(), true);
  assert.equal(await page.getByRole('button', { name: 'Store', exact: true }).count(), 0);
  // Native radio keyboard navigation.
  await page.getByRole('radio').first().focus();
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.getByRole('radio', { name: /^OpenClaw/ }).isChecked(), true);
  await page.getByRole('radio', { name: /^Hermes/ }).check();
  if (process.env.ONBOARDING_SCREENSHOT) await page.screenshot({ path: process.env.ONBOARDING_SCREENSHOT, fullPage: true });
  await choose('OpenClaw');
  assert.equal(await page.getByRole('button', { name: /Review .* installation/ }).count(), 0);
  assert.match(await page.locator('body').innerText(), /Install through the official project/);
  // Installed CLI detection is separate from being configured or connected.
  await app.evaluate(({ ipcMain }) => {
    globalThis.nativeAgentCalls = [];
    ipcMain.removeHandler('agent-availability');
    ipcMain.handle('agent-availability', () => ({ detected: true, launchable: true }));
    ipcMain.removeHandler('launch-agent');
    ipcMain.handle('launch-agent', (_event, id, action) => { globalThis.nativeAgentCalls.push({ id, action }); });
  });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.getByRole('button', { name: 'Open OpenClaw onboarding', exact: true }).click();
  await page.getByRole('status').waitFor();
  assert.deepEqual(await app.evaluate(() => globalThis.nativeAgentCalls), [{ id: 'openclaw', action: 'setup' }]);
  assert.equal(await page.getByRole('textbox').count(), 0); // No OpenAgora credential/configuration forms.
  await app.evaluate(({ ipcMain }) => {
    ipcMain.removeHandler('launch-agent');
    ipcMain.handle('launch-agent', () => { throw new Error('No terminal app was found'); });
  });
  await page.getByRole('button', { name: 'Open OpenClaw dashboard', exact: true }).click();
  await page.getByRole('alert').waitFor();
  assert.match(await page.getByRole('alert').innerText(), /No terminal app/);


  // A true application restart must bypass first-run onboarding.
  await app.close();
  await launch();
  await heading('Store');
  await page.getByRole('button', { name: /Personal agent/ }).click();
  await heading('OpenClaw');
  await page.getByRole('button', { name: 'Change agent' }).click();
  await choose('Hermes');
  await page.getByRole('button', { name: 'Review Hermes installation' }).click();
  await heading('Hermes');
  await page.getByRole('button', { name: 'Install', exact: true }).click();
  await page.getByRole('dialog', { name: 'Install Hermes?' }).waitFor();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Install', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Install', exact: true }).click();
  await heading('Personal agent');
  await page.getByRole('button', { name: 'Start and open Hermes', exact: true }).waitFor();

  await page.getByRole('button', { name: /Personal agent/ }).click();
  await page.getByRole('button', { name: 'Change agent' }).click();
  await page.getByRole('radio', { name: /^Goose/ }).check();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, 'hermes');

  // Force a filesystem failure and verify the chooser stays open with an error.
  await page.getByRole('button', { name: 'Change agent' }).click();
  fs.mkdirSync(`${file}.tmp`);
  await page.getByRole('button', { name: 'Continue with Hermes' }).click();
  await page.getByRole('alert').waitFor();
  await heading('Choose your personal agent');
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, 'hermes');
  fs.rmdirSync(`${file}.tmp`);
  await page.getByRole('button', { name: 'Use without an agent' }).click();
  await heading('Store');
  await page.reload();
  await heading('Store');
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, null);

  // Corrupt preference recovery and first-run skip.
  fs.writeFileSync(file, '{broken');
  await page.reload();
  await page.getByRole('button', { name: 'Retry', exact: true }).waitFor();
  assert.equal(fs.readFileSync(file, 'utf8'), '{broken');
  await page.getByRole('button', { name: 'Choose again' }).click();
  await heading('Choose your personal agent');
  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(900, 600));
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  await page.getByRole('radio', { name: /^ZeroClaw/ }).check();
  await page.getByRole('button', { name: 'Choose later' }).click();
  await heading('Store');
  await page.reload();
  await heading('Store');
  assert.deepEqual(JSON.parse(fs.readFileSync(file)), { version: 1, completed: true, selectedAgent: null });
  assert.deepEqual(errors, []);
  console.log('Onboarding UI passed: choice, restart, switch, cancel, install review, failed save, recovery, skip, keyboard, small window.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (app) await app.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

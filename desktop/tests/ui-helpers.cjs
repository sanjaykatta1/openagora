// Shared by the UI tests: launches the built app in an isolated profile and
// replaces the engine with an in-memory fake, so nothing is installed and no
// network or account is needed. Preferences IPC stays real.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { _electron: electron } = require('playwright-core');

/** A stand-in for Hermes's dashboard: answers every page and logs the request. */
async function fakeDashboard() {
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push(req.url);
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!doctype html><title>Hermes</title><h1>Fake Hermes dashboard</h1><p>${req.url.replace(/[<>&]/g, '')}</p>`);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  return { url: `http://127.0.0.1:${port}/`, requests, close: () => server.close() };
}

/** Where Hermes and Sunday stand in the fake engine. */
function freshFakeState(overrides = {}) {
  return {
    present: false, installed: false, adopted: false, clean: false, version: '0.21.6',
    setup: 'unknown', running: false, dashboardUrl: null, pid: null,
    calls: [], ...overrides,
  };
}

async function launch({ directory, fake }) {
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${directory}`, ...(process.platform === 'linux' ? ['--no-sandbox'] : [])],
    cwd: path.resolve(__dirname, '..'),
  });
  assert.equal(fs.realpathSync(await app.evaluate(({ app }) => app.getPath('userData'))), fs.realpathSync(directory));
  const page = await app.firstWindow();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  if (fake) await installFakeEngine(app, fake);
  await page.reload();
  return { app, page, errors };
}

/** Swaps the engine-backed IPC handlers for a fake that keeps its state in
 *  globalThis.fake inside the main process. */
async function installFakeEngine(app, state) {
  await app.evaluate(({ ipcMain }, initial) => {
    globalThis.fake = initial;
    const f = () => globalThis.fake;
    const listing = () => ({
      id: 'hermes', name: 'Hermes', summary: 'Powers Sunday', category: 'agents',
      homepage: 'https://hermes-agent.nousresearch.com', source: 'https://github.com/NousResearch/hermes-agent',
      license: 'MIT', platforms: ['macos', 'linux', 'windows'], available: true, role: 'agent', profile: 'sunday',
      ui: 'web', steps: [{ download: null, run: 'curl … | bash' }], permissions: [],
      installed: f().installed, adopted: f().adopted, present: f().present,
      status: f().running ? 'running' : f().installed ? 'stopped' : 'not-installed',
      pid: f().running ? f().pid : null, url: f().running ? f().dashboardUrl : null, home_url: f().running ? f().dashboardUrl : null,
    });
    const ok = { ok: true, stdout: '', stderr: '' };
    const handle = (channel, fn) => { ipcMain.removeHandler(channel); ipcMain.handle(channel, fn); };
    handle('catalog', () => [listing()]);
    handle('sunday-status', () => ({
      id: 'hermes', name: 'Hermes', profile: 'sunday',
      hermes: f().present ? '/home/test/.local/bin/hermes' : null,
      installed: f().installed, adopted: f().adopted, ask_how_to_start: !(f().installed && f().clean),
      version: f().present ? f().version : null, min_version: '0.21.3',
      version_ok: f().present ? f().version >= '0.21.3' : null,
      setup: f().present ? f().setup : 'unknown',
      profile_dir: f().setup === 'missing' ? null : '/home/test/.hermes/profiles/sunday',
      dashboard: f().running ? 'running' : 'stopped', url: f().running ? f().dashboardUrl : null, pid: f().running ? f().pid : null,
      gateway: f().running ? { running: false, state: 'stopped', platforms: {} } : null,
    }));
    handle('install', (_e, id) => {
      f().calls.push(['install', id]);
      f().adopted = f().present;
      f().clean = !f().present;
      f().installed = true; f().present = true;
      if (f().setup === 'unknown') f().setup = 'missing';
      return ok;
    });
    handle('sunday-setup', (_e, choice) => {
      f().calls.push(['sunday-setup', choice]);
      if (!f().installed) { f().installed = true; f().adopted = true; }
      f().setup = 'ready';
      return ok;
    });
    handle('start', (_e, id) => {
      f().calls.push(['start', id]);
      if (f().setup !== 'ready') return { ok: false, stdout: '', stderr: "Sunday isn't set up yet." };
      f().running = true; f().pid = (f().pid ?? 1000) + 1;
      return ok;
    });
    handle('stop', (_e, id) => { f().calls.push(['stop', id]); f().running = false; return ok; });
    handle('update-app', (_e, id) => { f().calls.push(['update-app', id]); f().version = '0.21.6'; return ok; });
    handle('uninstall', (_e, id) => {
      f().calls.push(['uninstall', id]);
      f().installed = false; f().adopted = false; f().running = false; f().setup = 'missing';
      return ok;
    });
    handle('agent-availability', () => ({ detected: true, launchable: true }));
    handle('launch-agent', (_e, id, action) => { f().calls.push(['launch-agent', id, action]); });
  }, state);
}

const fakeCalls = (app) => app.evaluate(() => globalThis.fake.calls);
const setFake = (app, changes) => app.evaluate((_electron, c) => Object.assign(globalThis.fake, c), changes);

function tempProfile(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

async function heading(page, text) {
  await page.getByRole('heading', { name: text, exact: true }).waitFor();
}

async function waitFor(check, what, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`timed out waiting for ${what}`);
}

module.exports = { fakeDashboard, freshFakeState, launch, fakeCalls, setFake, tempProfile, heading, waitFor };

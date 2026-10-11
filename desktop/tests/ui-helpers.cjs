// Shared by the UI tests: launches the built app in an isolated profile and
// replaces the engine with an in-memory fake, so nothing is installed and no
// network or account is needed. Preferences IPC stays real.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const net = require('node:net');
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

/** A stand-in for the engine service (`openagora serve`) that the window
 *  connects to: the test plays the engine's side of the conversation. */
async function fakeDaemon() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'oa-daemon-'));
  const address = process.platform === 'win32'
    ? `\\\\.\\pipe\\openagora-test-${process.pid}-${Date.now()}`
    : path.join(dir, 'engine.sock');
  const sockets = [];
  const received = [];
  const hellos = [];
  /** The engine's side of the window's requests: method → (params) => result. */
  const handlers = new Map();
  const server = net.createServer((socket) => {
    let buffer = '';
    socket.setEncoding('utf8');
    socket.on('data', (chunk) => {
      buffer += chunk;
      let i;
      while ((i = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, i).trim();
        buffer = buffer.slice(i + 1);
        if (!line) continue;
        const message = JSON.parse(line);
        if (message.hello) { sockets.push(socket); hellos.push(message.hello); continue; }
        received.push(message);
        const handler = message.method && message.id !== undefined ? handlers.get(message.method) : null;
        if (handler) {
          Promise.resolve().then(() => handler(message.params ?? {})).then(
            (result) => socket.write(JSON.stringify({ jsonrpc: '2.0', id: message.id, result }) + '\n'),
            (error) => socket.write(JSON.stringify({ jsonrpc: '2.0', id: message.id, error: { code: -32000, message: String(error.message ?? error) } }) + '\n'),
          );
        }
      }
    });
    socket.on('error', () => {});
  });
  await new Promise((resolve) => server.listen(address, resolve));
  let asked = 0;
  const api = {
    address,
    connected: () => sockets.length > 0,
    /** What the window said about itself when it connected, newest last. */
    hellos,
    /** Everything the window sent besides its hellos. */
    received,
    /** Answers the window's requests for `method`. */
    handle: (method, handler) => handlers.set(method, handler),
    send: (message) => sockets.at(-1).write(JSON.stringify({ jsonrpc: '2.0', ...message }) + '\n'),
    /** Asks the window something, as the engine does, and waits for the whole reply. */
    ask: async (method, params, timeout = 30000) => {
      const id = `engine-${++asked}`;
      api.send({ id, method, params });
      await waitFor(() => received.some((m) => m.id === id), `the window's answer to ${method}`, timeout);
      return received.find((m) => m.id === id);
    },
    /** The window's reply to request `id`, once it arrives. */
    reply: async (id) => {
      await waitFor(() => received.some((m) => m.id === id), `the window's answer to ${id}`, 30000);
      return received.find((m) => m.id === id);
    },
    close: () => { for (const s of sockets) s.destroy(); server.close(); fs.rmSync(dir, { recursive: true, force: true }); },
  };
  return api;
}

/** Where Hermes and Sunday stand in the fake engine. */
function freshFakeState(overrides = {}) {
  return {
    present: false, installed: false, adopted: false, clean: false, version: '0.21.6',
    setup: 'unknown', running: false, dashboardUrl: null, pid: null, connected: false,
    extraApps: [], calls: [], grants: [], never: [], ...overrides,
  };
}

async function launch({ directory, fake, daemon }) {
  // Never start a real engine service from a test: talk to a stand-in.
  const ownDaemon = daemon ? null : await fakeDaemon();
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${directory}`, ...(process.platform === 'linux' ? ['--no-sandbox'] : [])],
    cwd: path.resolve(__dirname, '..'),
    env: { ...process.env, OPENAGORA_TEST: '1', OPENAGORA_DAEMON_ADDRESS: (daemon ?? ownDaemon).address },
  });
  if (ownDaemon) app.on('close', () => ownDaemon.close());
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
    handle('catalog', () => [listing(), ...f().extraApps]);
    handle('sunday-status', () => ({
      id: 'hermes', name: 'Hermes', profile: 'sunday',
      hermes: f().present ? '/home/test/.local/bin/hermes' : null,
      installed: f().installed, adopted: f().adopted, ask_how_to_start: !(f().installed && f().clean),
      version: f().present ? f().version : null, min_version: '0.21.3',
      version_ok: f().present ? f().version >= '0.21.3' : null,
      setup: f().present ? f().setup : 'unknown',
      profile_dir: f().setup === 'missing' ? null : '/home/test/.hermes/profiles/sunday',
      connected: f().connected,
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
    handle('sunday-connect', () => { f().calls.push(['sunday-connect']); f().connected = true; return ok; });
    handle('sunday-disconnect', () => { f().calls.push(['sunday-disconnect']); f().connected = false; return ok; });
    handle('broker-state', () => ({
      grants: f().grants,
      never: { built_in: [{ name: '1Password', apps: ['1Password'] }, { name: 'Private browser windows', titles: ['Incognito'] }], user: f().never },
    }));
    handle('broker-revoke', (_e, app) => { f().calls.push(['broker-revoke', app]); f().grants = f().grants.filter((g) => g.source !== app); return ok; });
    handle('never-add', (_e, rule) => {
      f().calls.push(['never-add', rule]);
      f().never.push({ name: rule.name, apps: rule.app ? [rule.app] : [], titles: rule.title ? [rule.title] : [] });
      return ok;
    });
    handle('never-remove', (_e, name) => { f().calls.push(['never-remove', name]); f().never = f().never.filter((r) => r.name !== name); return ok; });
    handle('agent-availability', () => ({ detected: true, launchable: true }));
    handle('launch-agent', (_e, id, action) => { f().calls.push(['launch-agent', id, action]); });
  }, state);
}

const fakeCalls = (app) => app.evaluate(() => globalThis.fake.calls);

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

module.exports = { fakeDashboard, fakeDaemon, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor };

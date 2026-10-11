// OpenAgora's window and the real engine together (acceptance tests 6 and
// 10 end to end): the engine finds the app in front, checks the broker, has
// the window capture it through the OS, keeps the picture on the desk and
// tells Sunday, and Sunday's MCP tools see the same picture. Only the OS's
// window list is replaced (OPENAGORA_FOCUS_FILE); the app in front is a real
// window of another process. Linux (X11) only; skipped without an engine build.
const assert = require('node:assert/strict');
const { execFileSync, spawn } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');
const { _electron: electron } = require('playwright-core');
const { waitFor } = require('./ui-helpers.cjs');

const EXE = process.platform === 'win32' ? 'openagora.exe' : 'openagora';
const engine = [
  process.env.OPENAGORA_ENGINE,
  path.join(__dirname, '..', 'resources', 'bin', EXE),
  path.join(__dirname, '..', '..', 'engine', 'target', 'debug', EXE),
  path.join(__dirname, '..', '..', 'engine', 'target', 'release', EXE),
].find((p) => p && fs.existsSync(p));

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-real-engine-'));
const env = {
  ...process.env,
  HOME: path.join(root, 'home'),
  OPENAGORA_HOME: path.join(root, 'data'),
  OPENAGORA_FOCUS_FILE: path.join(root, 'focus.json'),
};
const oa = (...args) => execFileSync(engine, args, { env, encoding: 'utf8' });
let app;
let standin;

/** Sunday's MCP connection, as Hermes makes it. */
function mcpClient() {
  const child = spawn(engine, ['mcp', '--client', 'sunday'], { env, stdio: ['pipe', 'pipe', 'inherit'] });
  const replies = new Map();
  readline.createInterface({ input: child.stdout }).on('line', (line) => {
    const message = JSON.parse(line);
    if (message.id !== undefined && !message.method) replies.set(message.id, message);
  });
  let next = 1;
  const request = async (method, params) => {
    const id = next++;
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    await waitFor(() => replies.has(id), `the engine's answer to ${method}`, 60000);
    return replies.get(id);
  };
  return { child, request };
}

async function run() {
  if (process.platform !== 'linux' || !engine) {
    console.log('Real engine test skipped (needs Linux and an engine build).');
    return;
  }
  fs.mkdirSync(env.HOME);
  // Another app's window, in front.
  standin = spawn(require('electron'), [path.join(__dirname, 'fixtures', 'standin-app.cjs'), '--no-sandbox'], { stdio: ['ignore', 'pipe', 'ignore'] });
  const sourceId = await new Promise((resolve) => readline.createInterface({ input: standin.stdout }).once('line', resolve));
  const id = Number(sourceId.split(':')[1]);
  fs.writeFileSync(env.OPENAGORA_FOCUS_FILE, JSON.stringify([{ id, pid: standin.pid, app: 'Firefox', key: 'firefox', title: 'Docs — Mozilla Firefox' }]));

  app = await electron.launch({
    args: ['.', `--user-data-dir=${path.join(root, 'profile')}`, '--no-sandbox'],
    cwd: path.resolve(__dirname, '..'),
    env: { ...env, OPENAGORA_TEST: '1', OPENAGORA_ENGINE: engine },
  });
  await app.firstWindow();
  await waitFor(() => fs.existsSync(path.join(env.OPENAGORA_HOME, 'run', 'daemon.toml')), 'the window to start the engine service', 30000);
  // The window reports its own windows when it connects; give it a moment.
  await new Promise((r) => setTimeout(r, 1000));

  // Not allowed yet: the panel says so, and nothing is captured.
  await app.evaluate(() => globalThis.openagoraTest.pressShortcut());
  let panel;
  await waitFor(() => (panel = app.windows().find((w) => w.url().endsWith('#panel'))), 'the Sunday panel');
  await panel.getByText('Sunday can’t see Firefox yet.').waitFor();
  assert.deepEqual(JSON.parse(oa('desk', 'list', '--json')), []);

  // The user allows it in the panel: the engine has the window capture the
  // other app's window through the OS, and keeps it on the desk.
  await panel.getByRole('button', { name: 'Always allow' }).click();
  await panel.getByText('Sunday sees Firefox.').waitFor({ timeout: 60000 });
  const [item] = JSON.parse(oa('desk', 'list', '--json'));
  assert.equal(item.kind, 'screenshot');
  assert.equal(item.source.app, 'Firefox');
  assert.deepEqual(item.shared_with, ['sunday']);
  const picture = path.join(root, 'picture.png');
  oa('desk', 'show', item.id, '--out', picture);
  // It's the other app's window, not anything else on screen.
  const pixel = await app.evaluate(({ nativeImage }, file) => {
    const image = nativeImage.createFromPath(file);
    const { width, height } = image.getSize();
    const bitmap = image.toBitmap();
    const at = (Math.floor(height / 2) * width + Math.floor(width / 2)) * 4;
    return { width, height, bgra: [...bitmap.subarray(at, at + 4)] };
  }, picture);
  assert.equal(pixel.width, 400);
  assert.equal(pixel.height, 300);
  const [b, g, r] = pixel.bgra;
  assert.ok(Math.abs(r - 0xe0) < 24 && Math.abs(g - 0x40) < 24 && Math.abs(b - 0x20) < 24, `unexpected colour ${pixel.bgra}`);

  // Sunday's next reply hears about it.
  const hook = execFileSync(engine, ['context', '--hook', '--client', 'sunday'], {
    env, encoding: 'utf8', input: JSON.stringify({ session_id: 'real', extra: { is_first_turn: true, platform: 'tui' } }),
  });
  const told = JSON.parse(hook).context;
  assert.match(told, /pressed Sunday's shortcut over Firefox/);
  assert.ok(told.includes(item.id), told);

  // Sunday sees the same picture through its MCP tools, and can capture again.
  const sunday = mcpClient();
  try {
    await sunday.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test' } });
    const got = await sunday.request('tools/call', { name: 'desk_get', arguments: { id: item.id } });
    const image = got.result.content.find((c) => c.type === 'image');
    assert.equal(image.mimeType, 'image/png');
    assert.ok(Buffer.from(image.data, 'base64').equals(fs.readFileSync(picture)));
    const shot = await sunday.request('tools/call', { name: 'capture_window', arguments: {} });
    assert.equal(shot.result.isError, false, JSON.stringify(shot.result.content[0]));
    assert.ok(shot.result.content.some((c) => c.type === 'image'));
  } finally {
    sunday.child.kill();
  }

  // Every step is in the audit log.
  const audit = JSON.parse(oa('audit', '--json'));
  const actions = audit.map((e) => `${e.client}:${e.action}:${e.decision}`);
  for (const expected of ['user:capture:refused', 'user:allow_app:confirmed', 'user:capture:allowed', 'sunday:desk_get:allowed', 'sunday:capture:allowed']) {
    assert.ok(actions.includes(expected), `${expected} missing from ${actions}`);
  }
  console.log('Real engine passed: shortcut, broker, OS window capture, desk, hook, MCP desk_get and capture_window, audit.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (app) await app.close().catch(() => {});
  if (engine) { try { oa('daemon', 'stop'); } catch { /* not running */ } }
  if (standin) standin.kill();
  fs.rmSync(root, { recursive: true, force: true });
});

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
const { markNormal, raise } = require('./fixtures/x11-wm-state.cjs');

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
  standin = spawn(require('electron'), [path.join(__dirname, 'fixtures', 'standin-app.cjs'), '--no-sandbox'], { stdio: ['pipe', 'pipe', 'ignore'] });
  const lines = readline.createInterface({ input: standin.stdout });
  const nextLine = () => new Promise((resolve) => lines.once('line', resolve));
  const sourceId = await nextLine();
  const id = Number(sourceId.split(':')[1]);
  // Xvfb runs no window manager: mark the window as one would, so the OS's
  // window list (and OpenAgora's window) sees it with its title.
  if (process.platform === 'linux') await markNormal(id);
  // The engine sees the window but not its title, as on macOS when only
  // OpenAgora's window has Screen Recording permission.
  fs.writeFileSync(env.OPENAGORA_FOCUS_FILE, JSON.stringify([{ id, pid: standin.pid, app: 'Firefox', key: 'firefox' }]));

  app = await electron.launch({
    args: ['.', `--user-data-dir=${path.join(root, 'profile')}`, '--no-sandbox'],
    cwd: path.resolve(__dirname, '..'),
    env: { ...env, OPENAGORA_TEST: '1', OPENAGORA_ENGINE: engine },
  });
  await app.firstWindow();
  await waitFor(() => fs.existsSync(path.join(env.OPENAGORA_HOME, 'run', 'daemon.toml')), 'the window to start the engine service', 30000);
  // The window reports its own windows when it connects; give it a moment.
  await new Promise((r) => setTimeout(r, 1000));

  // The user works in the stand-in, so nothing covers it: OpenAgora's main
  // window moves aside (Xvfb has no window manager to keep the window the
  // user is in on top).
  await app.evaluate(({ BrowserWindow }) => {
    const main = BrowserWindow.getAllWindows().find((w) => w.webContents.getURL().endsWith('index.html'));
    main?.setBounds({ x: 470, y: 380, width: 900, height: 600 });
  });
  if (process.platform === 'linux') await raise(id);

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
  // The window's title as OpenAgora's window saw it while capturing.
  assert.equal(item.source.title, 'Docs — Mozilla Firefox');
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
    // The user opens a private window in the allowed app. The engine can't
    // see titles, but the title the window was captured with is checked
    // against the never-capture list, and the picture isn't kept.
    standin.stdin.write('title Bank — Mozilla Firefox Private Browsing\n');
    await nextLine();
    await new Promise((r) => setTimeout(r, 300));
    const before = JSON.parse(oa('desk', 'list', '--json')).length;
    const privately = await sunday.request('tools/call', { name: 'capture_window', arguments: {} });
    assert.equal(privately.result.isError, true, JSON.stringify(privately.result));
    assert.match(privately.result.content[0].text, /never-capture list/);
    assert.ok(!privately.result.content.some((c) => c.type === 'image'));
    assert.equal(JSON.parse(oa('desk', 'list', '--json')).length, before);
  } finally {
    sunday.child.kill();
  }

  // Every step is in the audit log.
  const audit = JSON.parse(oa('audit', '--json'));
  const actions = audit.map((e) => `${e.client}:${e.action}:${e.decision}`);
  for (const expected of ['user:capture:refused', 'user:allow_app:confirmed', 'user:capture:allowed', 'sunday:desk_get:allowed', 'sunday:capture:allowed', 'sunday:capture:refused']) {
    assert.ok(actions.includes(expected), `${expected} missing from ${actions}`);
  }
  console.log('Real engine passed: shortcut, broker, OS window capture, the title as captured, desk, hook, MCP desk_get and capture_window, a private window refused, audit.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (app) await app.close().catch(() => {});
  if (engine) { try { oa('daemon', 'stop'); } catch { /* not running */ } }
  if (standin) standin.kill();
  fs.rmSync(root, { recursive: true, force: true });
});

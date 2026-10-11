// Window handling with a real engine and a tiny local web app: the window can
// be moved from the top of any page or app tab, and a tab pops out into its own
// window that stays signed in.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { _electron: electron } = require('playwright-core');

const root = path.resolve(__dirname, '..');
const exe = process.platform === 'win32' ? 'openagora.exe' : 'openagora';
const engine = [
  process.env.OPENAGORA_ENGINE,
  path.join(root, 'resources', 'bin', exe),
  path.join(root, '..', 'engine', 'target', 'release', exe),
  path.join(root, '..', 'engine', 'target', 'debug', exe),
].find((p) => p && fs.existsSync(p));
if (!engine) throw new Error('Build the engine first (cargo build in engine/).');

const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-windows-'));
const catalog = path.join(directory, 'catalog');
fs.mkdirSync(path.join(catalog, 'apps', 'web'), { recursive: true });
const platform = { darwin: 'macos', win32: 'windows' }[process.platform] ?? 'linux';
fs.writeFileSync(path.join(catalog, 'apps', 'web', 'app.toml'), `
schema = "openagora/v1"
id = "web"
name = "Web demo"
summary = "A tiny web server"
category = "test"
homepage = "https://example.com"
source = "https://github.com/example/web"
license = "MIT"
platforms = ["${platform}"]

[install]
provides = "{app_dir}/index.html"

[[install.steps]]
os = ["${platform}"]
shell = "sh"
run = "echo '<title>Web demo</title><h1>hello-openagora</h1>' > '{app_dir}/index.html'"

[run]
command = "python3"
args = ["-m", "http.server", "{port}", "--bind", "127.0.0.1"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{port}/"
`);
const userData = path.join(directory, 'user-data');
fs.mkdirSync(userData);
fs.writeFileSync(path.join(userData, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: null }));

const appRegion = (page, selector) =>
  page.locator(selector).first().evaluate((el) => getComputedStyle(el).getPropertyValue('-webkit-app-region') || getComputedStyle(el).webkitAppRegion);

(async () => {
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${userData}`, ...(process.platform === 'linux' ? ['--no-sandbox'] : [])],
    cwd: root,
    env: { ...process.env, OPENAGORA_ENGINE: engine, OPENAGORA_CATALOG: catalog, OPENAGORA_HOME: path.join(directory, 'home') },
  });
  try {
    const page = await app.firstWindow();
    await page.getByRole('heading', { name: 'Store' }).waitFor();

    // A page: the strip across its top moves the window; the search box still works.
    assert.equal(await appRegion(page, '.drag-strip'), 'drag');
    assert.notEqual(await appRegion(page, '.search'), 'drag');
    await page.locator('.search').fill('web');

    await page.getByRole('button', { name: 'Install' }).first().click();
    await page.locator('.modal').getByRole('button', { name: 'Install' }).click();
    await page.getByRole('button', { name: 'Start' }).first().click({ timeout: 30000 });

    // An app tab: its top bar moves the window; its buttons stay clickable.
    const bar = page.locator('.webview-bar').first();
    await bar.waitFor({ timeout: 30000 });
    assert.equal(await page.locator('.drag-strip').count(), 0, 'the page strip must not cover a tab');
    assert.equal(await appRegion(page, '.webview-bar'), 'drag');
    assert.notEqual(await appRegion(page, '.webview-bar button'), 'drag');

    // Pop out: the app moves to its own window and leaves the sidebar.
    const opened = app.waitForEvent('window');
    await bar.getByRole('button', { name: 'Pop out' }).click();
    const popped = await opened;
    await popped.waitForLoadState();
    assert.match(popped.url(), /^http:\/\/127\.0\.0\.1:\d+\/$/);
    assert.match(await popped.locator('h1').textContent(), /hello-openagora/);
    assert.equal(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().map((w) => w.getTitle()).sort().join(',')), 'OpenAgora,Web demo');
    await page.locator('.sidebar .tab').waitFor({ state: 'detached' });

    // Popping out again focuses the same window rather than opening another.
    await page.getByRole('button', { name: 'Library' }).click();
    await page.getByRole('button', { name: 'Open' }).first().click();
    await page.locator('.webview-bar').getByRole('button', { name: 'Pop out' }).click();
    await page.locator('.sidebar .tab').waitFor({ state: 'detached' });
    assert.equal(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length), 2);

    // Stopping the app closes its window.
    await page.getByRole('button', { name: 'Stop' }).first().click();
    await page.waitForFunction(() => true);
    for (let i = 0; i < 50 && (await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)) > 1; i++) {
      await new Promise((r) => setTimeout(r, 200));
    }
    assert.equal(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length), 1);
    console.log('Window handling passed: drag strip, tab bar drag, pop out, refocus, close on stop.');
  } finally {
    await Promise.race([app.close(), new Promise((r) => setTimeout(r, 10000))]);
    fs.rmSync(directory, { recursive: true, force: true });
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});

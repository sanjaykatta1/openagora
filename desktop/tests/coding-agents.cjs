// Settings → Coding agents (acceptance test 7, the window's side): agents are
// set up only after the user sees the exact command OpenAgora will run, an
// agent without a removal command gets instructions instead, and T3 Code is
// connected on its own approval page, which OpenAgora's window shows with
// T3's own session and closes once T3 answers. The engine is a stand-in;
// Rust tests cover its side.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

let session;
let daemon;
let dashboard;
const directory = tempProfile('openagora-coding-');

const agent = (id, name, extra) => ({
  id, name, custom: false, installed: true, set_up: false, version: null,
  setup_command: `${id} mcp add openagora -- /data/bin/openagora mcp --client ${id}`,
  remove_command: `${id} mcp remove openagora`, remove_by_hand: null, ...extra,
});

async function run() {
  dashboard = await fakeDashboard();
  daemon = await fakeDaemon();
  fs.writeFileSync(path.join(directory, 'personal-agent.json'), JSON.stringify({ version: 1, completed: true, selectedAgent: 'hermes' }));
  const t3 = {
    id: 't3code', name: 'T3 Code', summary: 'Coding agents in threads', category: 'coding',
    homepage: 'https://t3.codes', source: 'https://github.com/pingdotgg/t3code', license: 'MIT',
    platforms: ['macos', 'linux', 'windows'], available: true, role: 'app', profile: null, ui: 'web',
    steps: [], permissions: [], installed: true, adopted: false, present: true, status: 'running',
    pid: 42, url: dashboard.url, home_url: dashboard.url,
    destination: { accepts: ['image/png', 'text/*'], deliver: 'mcp', adapter: 't3code' }, mcp_auth: 'oauth',
  };
  session = await launch({
    directory, daemon,
    fake: freshFakeState({
      present: true, installed: true, clean: true, setup: 'ready', running: true, pid: 7, connected: true,
      dashboardUrl: dashboard.url, extraApps: [t3],
      agents: [
        agent('claude-code', 'Claude Code', {
          setup_command: 'claude mcp add --scope user --transport stdio openagora -- /data/bin/openagora mcp --client claude-code',
          hook_commands: ['claude plugin marketplace add /data/claude-code/marketplace --scope user', 'claude plugin install openagora@openagora --scope user'],
        }),
        agent('codex', 'Codex', { installed: false }),
        agent('opencode', 'OpenCode', { set_up: true, remove_command: null, remove_by_hand: 'OpenCode has no command to remove an MCP server: remove "openagora" from the "mcp" section of its opencode.json yourself.' }),
      ],
    }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');
  await page.getByRole('button', { name: /^Settings/ }).click();
  await heading(page, 'Coding agents');
  const agents = page.getByRole('list', { name: 'Coding agents' });
  const row = (name) => agents.getByRole('listitem').filter({ hasText: name });
  assert.match(await row('Codex').innerText(), /not on this computer/);
  assert.equal(await row('Codex').getByRole('button').count(), 0);

  // Setting up Claude Code shows the exact commands first; nothing runs
  // until the user says so.
  await row('Claude Code').getByRole('button', { name: 'Set up…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Set up Claude Code?' });
  await dialog.waitFor();
  const shown = await dialog.innerText();
  assert.match(shown, /claude mcp add --scope user --transport stdio openagora -- \/data\/bin\/openagora mcp --client claude-code/);
  assert.match(shown, /claude plugin install openagora@openagora --scope user/);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  assert.deepEqual(await fakeCalls(app), []);
  await row('Claude Code').getByRole('button', { name: 'Set up…' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Run it' }).click();
  await page.getByRole('status').filter({ hasText: 'Claude Code can now read what Sunday sends it.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['agent-setup', 'claude-code']);
  await waitFor(async () => /set up: it reads what Sunday sends it/.test(await row('Claude Code').innerText()), 'Claude Code to show as set up');

  // OpenCode has no command to remove a server: OpenAgora says how, and
  // changes nothing itself.
  await row('OpenCode').getByRole('button', { name: 'Remove…' }).click();
  const manual = page.getByRole('dialog', { name: 'Remove OpenAgora from OpenCode' });
  assert.match(await manual.innerText(), /opencode\.json/);
  assert.equal(await manual.getByRole('button', { name: 'Remove' }).count(), 0);
  await manual.getByRole('button', { name: 'Close' }).click();
  assert.ok(!(await fakeCalls(app)).some((c) => c[0] === 'agent-remove'));

  // T3 Code: connecting opens T3's own approval page.
  const apps = page.getByRole('list', { name: 'Apps that take work' });
  await apps.getByRole('listitem').filter({ hasText: 'T3 Code' }).getByRole('button', { name: 'Connect…' }).click();
  const connect = page.getByRole('dialog', { name: 'Connect T3 Code?' });
  assert.match(await connect.innerText(), /Choose Supervised/);
  await connect.getByRole('button', { name: 'Open T3 Code’s page' }).click();
  await page.getByRole('status').filter({ hasText: 'OpenAgora is connected to T3 Code' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['app-connect', 't3code']);
  await apps.getByRole('button', { name: 'Disconnect' }).click();
  await page.getByRole('status').filter({ hasText: 'OpenAgora forgot its sign-in to T3 Code.' }).waitFor();

  // The engine has the window show an app's approval page: in a window with
  // the app's own tab session, only on this computer, closed once the app
  // sends the user back to the engine.
  const approve = `${dashboard.url}oauth/mcp/authorize?client_id=c&state=s`;
  const opened = await daemon.ask('oauth/open', { app: 't3code', url: approve });
  assert.deepEqual(opened.result, { opened: true }, JSON.stringify(opened));
  await waitFor(() => app.windows().some((w) => w.url().startsWith(approve)), 'the approval window');
  const sameSession = await app.evaluate(({ BrowserWindow, session }, url) => {
    const window = BrowserWindow.getAllWindows().find((w) => w.webContents.getURL().startsWith(url));
    return !!window && window.webContents.session === session.fromPartition('persist:app-t3code');
  }, approve);
  assert.equal(sameSession, true, 'the approval page uses the T3 tab’s session');
  await app.evaluate(({ BrowserWindow }, url) => {
    const window = BrowserWindow.getAllWindows().find((w) => w.webContents.getURL().startsWith(url));
    void window.loadURL(url.replace(/oauth\/mcp\/authorize.*$/, 'callback?code=c1&state=s'));
  }, approve);
  await waitFor(() => !app.windows().some((w) => w.url().includes('/callback') || w.url().startsWith(approve)), 'the approval window to close', 10000);
  const elsewhere = await daemon.ask('oauth/open', { app: 't3code', url: 'https://example.com/oauth' });
  assert.match(elsewhere.error.message, /isn't an approval page on this computer/);
  assert.deepEqual(session.errors, []);
  console.log('Coding agents UI passed: set up with the exact command, manual removal, T3 Code connect, approval window with the app’s session.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

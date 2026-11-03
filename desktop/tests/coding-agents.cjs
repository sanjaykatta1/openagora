// Settings → Coding agents (acceptance test 7, the window's side): agents are
// connected only after the user sees the exact command OpenAgora will run, an
// agent without a removal command gets instructions instead, and T3 Code is
// connected on its own approval page, which OpenAgora's window shows with
// T3's own session and closes once T3 answers. Settings → Memory (tests 11
// and 12, the window's side): Holographic and the digest only after the user
// sees what runs, and sessions Sunday may read taken back one by one.
// Settings → Activity: the audit log, newest first, filtered by who and by
// words. The engine is a stand-in; Rust tests cover its side.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { fakeDaemon, fakeDashboard, freshFakeState, launch, fakeCalls, tempProfile, waitFor } = require('./ui-helpers.cjs');

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
      memory: {
        provider: null, digest: false,
        providers: [{ id: 'holographic', name: 'Holographic', needs: '0.21.5', commands: [
          'hermes -p sunday plugins install holographic --enable --yes-deps', 'hermes -p sunday memory setup holographic',
        ] }],
        sessions: [{ agent: 'claude-code', name: 'Claude Code', always: false }, { agent: 'codex', name: 'Codex', always: false }],
      },
      audit: [
        { at: 1760000000, client: 'sunday', action: 'capture', target: 'firefox', decision: 'confirmed', via: 'window', outcome: 'ok' },
        { at: 1760000100, client: 'claude-code', action: 'context_get', target: 'ctx-1', decision: 'allowed', outcome: 'ok' },
        { at: 1760000200, client: 'sunday', action: 'sessions_read', target: 'codex', decision: 'declined', via: 'window', outcome: 'not read' },
      ],
    }),
  });
  const { app, page } = session;
  await waitFor(() => daemon.connected(), 'the window to connect to the engine');
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('tab', { name: 'Coding agents' }).click();
  const agents = page.getByRole('list', { name: 'Coding agents' });
  const row = (name) => agents.getByRole('listitem').filter({ hasText: name });
  assert.match(await row('Codex').innerText(), /Not installed/);
  assert.equal(await row('Codex').getByRole('button').count(), 0);

  // Connecting Claude Code shows the exact commands first; nothing runs
  // until the user says so.
  await row('Claude Code').getByRole('button', { name: 'Connect…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Connect Claude Code?' });
  await dialog.waitFor();
  const shown = await dialog.innerText();
  assert.match(shown, /claude mcp add --scope user --transport stdio openagora -- \/data\/bin\/openagora mcp --client claude-code/);
  assert.match(shown, /claude plugin install openagora@openagora --scope user/);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  assert.deepEqual(await fakeCalls(app), []);
  await row('Claude Code').getByRole('button', { name: 'Connect…' }).click();
  await dialog.getByRole('button', { name: 'Connect', exact: true }).click();
  await row('Claude Code').getByRole('button', { name: 'Disconnect…' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['agent-setup', 'claude-code']);

  // OpenCode has no command to remove a server: OpenAgora says how, and
  // changes nothing itself.
  await row('OpenCode').getByRole('button', { name: 'Disconnect…' }).click();
  const manual = page.getByRole('dialog', { name: 'Disconnect OpenCode' });
  assert.match(await manual.innerText(), /opencode\.json/);
  assert.equal(await manual.getByRole('button', { name: 'Disconnect' }).count(), 0);
  await manual.getByRole('button', { name: 'Close' }).click();
  assert.ok(!(await fakeCalls(app)).some((c) => c[0] === 'agent-remove'));

  // T3 Code: connecting opens T3's own approval page.
  const apps = page.getByRole('list', { name: 'Apps', exact: true });
  await apps.getByRole('listitem').filter({ hasText: 'T3 Code' }).getByRole('button', { name: 'Connect…' }).click();
  const connect = page.getByRole('dialog', { name: 'Connect T3 Code?' });
  assert.match(await connect.innerText(), /Choose Supervised/);
  await connect.getByRole('button', { name: 'Open T3 Code' }).click();
  await apps.getByRole('button', { name: 'Disconnect' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['app-connect', 't3code']);
  assert.equal(await apps.getByRole('button', { name: 'Connect…' }).count(), 0);
  await apps.getByRole('button', { name: 'Disconnect' }).click();
  await apps.getByRole('button', { name: 'Connect…' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['app-disconnect', 't3code']);

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

  // Settings → Memory: Holographic only after the user sees Hermes's own
  // commands, and back to built-in memory.
  await page.getByRole('tab', { name: 'Memory' }).click();
  const memory = page.getByRole('list', { name: 'Memory', exact: true });
  const memoryRow = (name) => memory.getByRole('listitem').filter({ hasText: name });
  const last = async () => (await fakeCalls(app)).at(-1);
  assert.match(await memoryRow('Long-term memory').innerText(), /Built-in/);
  await memoryRow('Long-term memory').getByRole('button', { name: 'Use Holographic…' }).click();
  const use = page.getByRole('dialog', { name: 'Use Holographic for long-term memory?' });
  const told = await use.innerText();
  assert.match(told, /hermes -p sunday plugins install holographic --enable --yes-deps/);
  assert.match(told, /hermes -p sunday memory setup holographic/);
  await use.getByRole('button', { name: 'Cancel' }).click();
  assert.ok(!(await fakeCalls(app)).some((c) => c[0] === 'memory-use'));
  await memoryRow('Long-term memory').getByRole('button', { name: 'Use Holographic…' }).click();
  await use.getByRole('button', { name: 'Use Holographic' }).click();
  await memoryRow('Long-term memory').getByRole('button', { name: 'Turn off…' }).waitFor();
  assert.deepEqual(await last(), ['memory-use', 'holographic']);
  assert.match(await memoryRow('Long-term memory').innerText(), /Holographic/);
  await memoryRow('Long-term memory').getByRole('button', { name: 'Turn off…' }).click();
  const off = page.getByRole('dialog', { name: 'Back to built-in memory?' });
  assert.match(await off.innerText(), /hermes -p sunday memory off/);
  await off.getByRole('button', { name: 'Turn off' }).click();
  await memoryRow('Long-term memory').getByRole('button', { name: 'Use Holographic…' }).waitFor();
  assert.deepEqual(await last(), ['memory-off']);

  // The digest says what it reads, and what it costs, before it is on.
  const sessions = page.getByRole('list', { name: 'Coding sessions Sunday can read' });
  assert.match(await sessions.innerText(), /Claude Code\s+Asks first/);
  const digestSwitch = page.getByRole('checkbox', { name: 'Digest' });
  await digestSwitch.click();
  const digest = page.getByRole('dialog', { name: 'Keep Sunday’s memory up to date?' });
  const explained = await digest.innerText();
  assert.match(explained, /in Claude Code, Codex/);
  assert.match(explained, /-s openagora-memory -t memory,mcp-openagora/);
  assert.match(explained, /Each run uses one model turn/);
  assert.equal(await digestSwitch.isChecked(), false);
  await digest.getByRole('button', { name: 'Turn on' }).click();
  await waitFor(() => digestSwitch.isChecked(), 'the digest to show as on');
  assert.deepEqual(await last(), ['memory-digest', true]);
  await waitFor(async () => /Codex\s+Always/.test(await sessions.innerText()), 'the sessions to show as always allowed');
  await sessions.getByRole('listitem').filter({ hasText: 'Codex' }).getByRole('button', { name: 'Take back' }).click();
  await waitFor(async () => /Codex\s+Asks first/.test(await sessions.innerText()), 'Codex to ask first again');
  assert.deepEqual(await last(), ['sessions-revoke', 'codex']);
  await digestSwitch.click();
  await waitFor(async () => !(await digestSwitch.isChecked()), 'the digest to show as off');
  assert.deepEqual(await last(), ['memory-digest', false]);

  // Settings → Activity: the audit log, newest first, by who and by words.
  await page.getByRole('tab', { name: 'Activity' }).click();
  const log = page.getByRole('list', { name: 'Audit log' });
  const lines = async () => (await log.getByRole('listitem').allInnerTexts()).map((t) => t.replace(/\s+/g, ' '));
  let listed = await lines();
  assert.equal(listed.length, 3);
  assert.match(listed[0], /^Sunday read coding sessions: codex — you said no in OpenAgora's window; not read/);
  assert.match(listed[1], /^Claude Code read what Sunday sent: ctx-1 — allowed/);
  assert.match(listed[2], /^Sunday capture a window: firefox — you said yes in OpenAgora's window/);
  await page.getByRole('combobox', { name: 'Who' }).selectOption({ label: 'Claude Code' });
  listed = await lines();
  assert.equal(listed.length, 1);
  assert.match(listed[0], /^Claude Code/);
  await page.getByRole('combobox', { name: 'Who' }).selectOption({ label: 'Everyone' });
  await page.getByRole('searchbox', { name: 'Search the activity' }).fill('firefox');
  listed = await lines();
  assert.equal(listed.length, 1);
  assert.match(listed[0], /firefox/);
  await page.getByRole('searchbox', { name: 'Search the activity' }).fill('');
  await app.evaluate(() => globalThis.fake.audit.push({ at: 1760000300, client: 'openagora', action: 'memory_digest', target: 'run', decision: 'allowed', outcome: '2 sessions: Saved 2 decisions to memory.' }));
  await page.getByRole('button', { name: 'Refresh' }).click();
  await waitFor(async () => (await lines()).length === 4, 'the new entry to show');
  assert.match((await lines())[0], /^OpenAgora memory digest: run — allowed; 2 sessions: Saved 2 decisions to memory\./);
  assert.deepEqual(session.errors, []);
  console.log('Coding agents, memory and activity UI passed: set up with the exact command, manual removal, T3 Code connect, approval window with the app’s session, Holographic, digest, sessions taken back, the audit log newest first and filtered.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  if (daemon) daemon.close();
  if (dashboard) dashboard.close();
  fs.rmSync(directory, { recursive: true, force: true });
});

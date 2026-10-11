// Settings → Coding agents (acceptance test 7, the window's side): agents are
// set up only after the user sees the exact command OpenAgora will run, an
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

  // Settings → Memory: Holographic only after the user sees Hermes's own
  // commands, and back to built-in memory.
  await heading(page, 'Memory');
  const memory = page.getByRole('list', { name: "Sunday's memory" });
  const memoryRow = (name) => memory.getByRole('listitem').filter({ hasText: name });
  assert.match(await memoryRow('Long-term memory').innerText(), /built-in only/);
  await memoryRow('Long-term memory').getByRole('button', { name: 'Use Holographic…' }).click();
  const use = page.getByRole('dialog', { name: 'Give Sunday long-term memory with Holographic?' });
  const told = await use.innerText();
  assert.match(told, /hermes -p sunday plugins install holographic --enable --yes-deps/);
  assert.match(told, /hermes -p sunday memory setup holographic/);
  assert.match(told, /no account, network or model calls/);
  await use.getByRole('button', { name: 'Cancel' }).click();
  assert.ok(!(await fakeCalls(app)).some((c) => c[0] === 'memory-use'));
  await memoryRow('Long-term memory').getByRole('button', { name: 'Use Holographic…' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Use Holographic' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday now remembers with Holographic.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['memory-use', 'holographic']);
  await waitFor(async () => /Holographic: facts in a local database/.test(await memoryRow('Long-term memory').innerText()), 'Holographic to show');
  await memoryRow('Long-term memory').getByRole('button', { name: 'Turn off…' }).click();
  assert.match(await page.getByRole('dialog', { name: 'Back to built-in memory?' }).innerText(), /hermes -p sunday memory off/);
  await page.getByRole('dialog').getByRole('button', { name: 'Turn off' }).click();
  await page.getByRole('status').filter({ hasText: 'built-in memory only' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['memory-off']);

  // The digest says what it reads, and what it costs, before it is on.
  const sessions = page.getByRole('list', { name: 'Coding sessions Sunday may read' });
  assert.match(await sessions.innerText(), /Claude Code\s+asks you first/);
  await memoryRow('Digest').getByRole('button', { name: 'Turn on…' }).click();
  const digest = page.getByRole('dialog', { name: 'Keep Sunday’s memory up to date?' });
  const explained = await digest.innerText();
  assert.match(explained, /in Claude Code, Codex/);
  assert.match(explained, /-s openagora-memory -t memory,mcp-openagora/);
  assert.match(explained, /each run costs one turn/);
  await digest.getByRole('button', { name: 'Turn on' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday’s memory digest is on.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['memory-digest', true]);
  await waitFor(async () => /Codex\s+always/.test(await sessions.innerText()), 'the sessions to show as always allowed');
  await sessions.getByRole('listitem').filter({ hasText: 'Codex' }).getByRole('button', { name: 'Take back' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday asks you before reading your Codex sessions again.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['sessions-revoke', 'codex']);
  await memoryRow('Digest').getByRole('button', { name: 'Turn off' }).click();
  await page.getByRole('status').filter({ hasText: 'Sunday’s memory digest is off.' }).waitFor();
  assert.deepEqual((await fakeCalls(app)).at(-1), ['memory-digest', false]);

  // Settings → Activity: the audit log, newest first, by who and by words.
  await heading(page, 'Activity');
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

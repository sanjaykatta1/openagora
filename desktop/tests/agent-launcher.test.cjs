const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { agentWorkflow, agentCommand } = require('../dist/electron/agent-workflows');
const { PERSONAL_AGENTS } = require('../dist/electron/agents');
const { findCommand, nativeCommand, launchAgent } = require('../dist/electron/agent-launcher');

function directory(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "openagora agent's $test-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test('every choice has an upstream interface; unsupported IDs and arbitrary commands fail closed', async (t) => {
  for (const agent of PERSONAL_AGENTS) assert.ok(agentWorkflow(agent.id).note);
  for (const id of ['__proto__', 'constructor', '../hermes', null]) assert.throws(() => agentWorkflow(id));
  for (const action of ['__proto__', 'constructor', 'setup; echo injected', ['setup']]) {
    assert.throws(() => agentCommand('hermes', action));
  }
  const dir = directory(t);
  await assert.rejects(launchAgent('hermes', 'arbitrary-shell', dir), /Unsupported/);
  assert.deepEqual(fs.readdirSync(dir), []);
  assert.deepEqual(agentCommand('hermes', 'setup').args, ['setup']);
  assert.deepEqual(agentCommand('openclaw', 'setup').args, ['onboard']);
});

test('detection requires a real executable and supports Windows launchers', (t) => {
  const dir = directory(t);
  fs.mkdirSync(path.join(dir, 'hermes'));
  assert.equal(findCommand('hermes', [dir]), undefined);
  fs.writeFileSync(path.join(dir, 'hermes.cmd'), '@echo off\n');
  assert.equal(findCommand('hermes', [dir], 'win32'), path.join(dir, 'hermes.cmd'));
  fs.writeFileSync(path.join(dir, 'goose'), '');
  if (process.platform !== 'win32') {
    assert.equal(findCommand('goose', [dir], 'darwin'), undefined);
    fs.chmodSync(path.join(dir, 'goose'), 0o700);
    assert.equal(findCommand('goose', [dir], 'darwin'), path.join(dir, 'goose'));
  }
});

test('native command preserves paths with spaces, quotes, and shell metacharacters', (t) => {
  const dir = directory(t);
  const args = ['setup', 'a b', "single'quote", '$(echo unwanted)', '$HOME', '; echo unsafe'];
  const script = path.join(dir, 'record.cjs');
  fs.writeFileSync(script, 'process.stdout.write(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd()}))');
  const command = nativeCommand(process.execPath, [script, ...args], dir);
  const actual = process.platform === 'win32'
    ? execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(command, 'utf16le').toString('base64')], { encoding: 'utf8' })
    : execFileSync('/bin/sh', ['-c', command], { encoding: 'utf8' });
  const result = JSON.parse(actual);
  assert.deepEqual(result.args, args);
  assert.equal(fs.realpathSync(result.cwd), fs.realpathSync(dir));
});

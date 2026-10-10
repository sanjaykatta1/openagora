const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { readAgentPreferences, saveAgentPreferences } = require('../dist/electron/preferences');
const { PERSONAL_AGENTS } = require('../dist/electron/agents');

function profile(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-preferences-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return directory;
}

test('first launch has no default; choosing, switching, and skipping survive restarts', (t) => {
  const directory = profile(t);
  assert.deepEqual(readAgentPreferences(directory), { version: 1, completed: false, selectedAgent: null });
  for (const selectedAgent of [...PERSONAL_AGENTS.map((a) => a.id), null]) {
    const value = { version: 1, completed: true, selectedAgent };
    saveAgentPreferences(directory, value);
    assert.deepEqual(readAgentPreferences(directory), value);
    assert.deepEqual(fs.readdirSync(directory), ['personal-agent.json']);
  }
});

test('malformed IPC input cannot overwrite an existing preference', (t) => {
  const directory = profile(t);
  const saved = { version: 1, completed: true, selectedAgent: 'openclaw' };
  saveAgentPreferences(directory, saved);
  for (const invalid of [null, {}, 'hermes', { ...saved, selectedAgent: '../other' },
    { ...saved, completed: 'true' }, { ...saved, completed: false }, { ...saved, version: 2 }]) {
    assert.throws(() => saveAgentPreferences(directory, invalid));
    assert.deepEqual(readAgentPreferences(directory), saved);
  }
});

test('corrupt data is reported and retained until the user replaces it', (t) => {
  const directory = profile(t);
  const file = path.join(directory, 'personal-agent.json');
  fs.writeFileSync(file, '{broken');
  assert.throws(() => readAgentPreferences(directory));
  assert.equal(fs.readFileSync(file, 'utf8'), '{broken');
  const value = { version: 1, completed: true, selectedAgent: 'hermes' };
  saveAgentPreferences(directory, value);
  assert.deepEqual(readAgentPreferences(directory), value);
});

test('a failed atomic replacement preserves the destination and removes the temporary file', (t) => {
  const directory = profile(t);
  const destination = path.join(directory, 'personal-agent.json');
  fs.mkdirSync(destination);
  fs.writeFileSync(path.join(destination, 'keep'), 'original');
  assert.throws(() => saveAgentPreferences(directory, { version: 1, completed: true, selectedAgent: null }));
  assert.equal(fs.readFileSync(path.join(destination, 'keep'), 'utf8'), 'original');
  assert.equal(fs.existsSync(`${destination}.tmp`), false);
});

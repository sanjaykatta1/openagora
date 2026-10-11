const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { readAgentPreferences, saveAgentPreferences } = require('../dist/electron/preferences');
const { PERSONAL_AGENTS, selectableAgents, parseAgentPreferences } = require('../dist/electron/agents');

function profile(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openagora-preferences-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return directory;
}

test('first launch has no default; choosing, switching, and skipping survive restarts', (t) => {
  const directory = profile(t);
  assert.deepEqual(readAgentPreferences(directory), { version: 1, completed: false, selectedAgent: null });
  for (const selectedAgent of [...selectableAgents().map((a) => a.id), null]) {
    const value = { version: 1, completed: true, selectedAgent };
    saveAgentPreferences(directory, value);
    assert.deepEqual(readAgentPreferences(directory), value);
    assert.deepEqual(fs.readdirSync(directory), ['personal-agent.json']);
  }
});

test('Sunday (Hermes) is the only selectable agent; the others are coming soon', () => {
  assert.deepEqual(selectableAgents().map((a) => a.id), ['hermes']);
  assert.equal(PERSONAL_AGENTS.find((a) => a.id === 'hermes').name, 'Sunday');
  for (const id of ['openclaw', 'goose', 'nanobot', 'nanoclaw', 'picoclaw', 'zeroclaw']) {
    assert.equal(PERSONAL_AGENTS.find((a) => a.id === id).comingSoon, true, id);
  }
});

test('an earlier choice of a coming-soon agent shows the chooser again, without an error', (t) => {
  const directory = profile(t);
  const file = path.join(directory, 'personal-agent.json');
  fs.writeFileSync(file, JSON.stringify({ version: 1, completed: true, selectedAgent: 'openclaw' }));
  assert.deepEqual(readAgentPreferences(directory), { version: 1, completed: false, selectedAgent: null });
  assert.deepEqual(parseAgentPreferences({ version: 1, completed: true, selectedAgent: 'goose' }),
    { version: 1, completed: false, selectedAgent: null });
  // A coming-soon agent can't be saved as the choice.
  assert.throws(() => saveAgentPreferences(directory, { version: 1, completed: true, selectedAgent: 'openclaw' }));
  assert.equal(JSON.parse(fs.readFileSync(file, 'utf8')).selectedAgent, 'openclaw');
});

test('malformed IPC input cannot overwrite an existing preference', (t) => {
  const directory = profile(t);
  const saved = { version: 1, completed: true, selectedAgent: 'hermes' };
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

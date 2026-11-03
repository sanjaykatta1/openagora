// First launch: Sunday's home asks whether to use Sunday, with the real
// preferences IPC in an isolated Electron profile. The engine is faked;
// nothing is installed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { freshFakeState, launch, tempProfile, heading, waitFor } = require('./ui-helpers.cjs');

const directory = tempProfile('openagora-onboarding-');
const file = path.join(directory, 'personal-agent.json');
let session;

async function start() {
  session = await launch({ directory, fake: freshFakeState() });
  return session.page;
}

const saved = () => JSON.parse(fs.readFileSync(file, 'utf8'));

async function run() {
  let page = await start();
  await heading(page, 'Sunday');
  // On Linux the window carries OpenAgora's icon itself (Windows and macOS
  // show the packaged app's).
  if (process.platform === 'linux') {
    const icon = await session.app.evaluate(({ nativeImage }) => {
      const file = globalThis.openagoraTest.windowIcon();
      return { file, size: nativeImage.createFromPath(file).getSize() };
    });
    assert.deepEqual(icon, { file: path.resolve(__dirname, '..', 'build', 'icons', '256x256.png'), size: { width: 256, height: 256 } });
  }
  // Nothing is chosen yet: Sunday, credited to its makers, or not now.
  await page.getByRole('button', { name: 'Use Sunday' }).waitFor();
  await page.getByRole('button', { name: 'Not now' }).waitFor();
  assert.match(await page.locator('body').innerText(), /Hermes Agent by Nous Research/);
  assert.equal(fs.existsSync(file), false);
  if (process.env.ONBOARDING_SCREENSHOT) await page.screenshot({ path: process.env.ONBOARDING_SCREENSHOT, fullPage: true });

  // A small window never scrolls sideways.
  await session.app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(900, 600));
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);

  // "Not now" is remembered across a real restart, and the Store becomes home.
  await page.getByRole('button', { name: 'Not now' }).click();
  await heading(page, 'Store');
  assert.deepEqual(saved(), { version: 1, completed: true, selectedAgent: null });
  await session.app.close();
  page = await start();
  await heading(page, 'Store');

  // Sunday's page still offers Sunday, without asking again; the keyboard
  // chooses it, and the choice survives a restart.
  await page.getByRole('button', { name: /^Sunday/ }).click();
  await page.getByRole('button', { name: 'Use Sunday' }).focus();
  assert.equal(await page.getByRole('button', { name: 'Not now' }).count(), 0);
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Install Hermes' }).waitFor();
  assert.equal(saved().selectedAgent, 'hermes');
  await session.app.close();
  page = await start();
  await page.getByRole('button', { name: 'Install Hermes' }).waitFor();

  // A failed save says so and keeps the old choice on disk.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('tab', { name: 'Sunday' }).click();
  fs.mkdirSync(`${file}.tmp`);
  await page.getByRole('button', { name: 'Stop using Sunday' }).click();
  await page.getByRole('alert').filter({ hasText: 'Couldn’t save your choice.' }).waitFor();
  assert.equal(saved().selectedAgent, 'hermes');
  fs.rmdirSync(`${file}.tmp`);
  // Stopping takes Sunday's tabs out of Settings.
  await page.getByRole('button', { name: 'Stop using Sunday' }).click();
  await heading(page, 'Store');
  assert.equal(saved().selectedAgent, null);
  await page.getByRole('button', { name: /^Settings/ }).click();
  assert.deepEqual(await page.getByRole('tab').allInnerTexts(), ['General', 'Activity']);

  // An earlier choice of an agent OpenAgora no longer offers just asks again.
  fs.writeFileSync(file, JSON.stringify({ version: 1, completed: true, selectedAgent: 'openclaw' }));
  await page.reload();
  await page.getByRole('button', { name: 'Not now' }).waitFor();
  assert.equal(await page.getByRole('alert').count(), 0);

  // Corrupt preferences are reported, kept, and replaced only by a new answer.
  fs.writeFileSync(file, '{broken');
  await page.reload();
  await page.getByRole('alert').filter({ hasText: 'Couldn’t read your saved choice' }).waitFor();
  assert.equal(fs.readFileSync(file, 'utf8'), '{broken');
  await page.getByRole('button', { name: 'Not now' }).click();
  await heading(page, 'Store');
  await waitFor(() => fs.readFileSync(file, 'utf8') !== '{broken', 'the new answer to be saved');
  assert.deepEqual(saved(), { version: 1, completed: true, selectedAgent: null });
  assert.deepEqual(session.errors, []);
  console.log('Onboarding UI passed: ask first, credit, small window, not now, restart, keyboard, stop using, failed save, retired choice, recovery.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  fs.rmSync(directory, { recursive: true, force: true });
});

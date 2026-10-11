// First launch: the personal-agent chooser, with the real preferences IPC in an
// isolated Electron profile. The engine is faked; nothing is installed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { freshFakeState, launch, tempProfile, heading } = require('./ui-helpers.cjs');

const directory = tempProfile('openagora-onboarding-');
const file = path.join(directory, 'personal-agent.json');
let session;

async function start() {
  session = await launch({ directory, fake: freshFakeState() });
  return session.page;
}

async function run() {
  let page = await start();
  await heading(page, 'Choose your personal agent');
  // On Linux the window carries OpenAgora's icon itself (Windows and macOS
  // show the packaged app's).
  if (process.platform === 'linux') {
    const icon = await session.app.evaluate(({ nativeImage }) => {
      const file = globalThis.openagoraTest.windowIcon();
      return { file, size: nativeImage.createFromPath(file).getSize() };
    });
    assert.deepEqual(icon, { file: path.resolve(__dirname, '..', 'build', 'icons', '256x256.png'), size: { width: 256, height: 256 } });
  }
  // Sunday is the only choice for now; every other agent says "Coming soon".
  assert.equal(await page.getByRole('radio').count(), 7);
  assert.equal(await page.locator('input[type=radio]:not([disabled])').count(), 1);
  assert.equal(await page.getByText('Coming soon', { exact: true }).count(), 6);
  assert.equal(await page.getByRole('radio', { name: /^OpenClaw/ }).isDisabled(), true);
  assert.equal(await page.locator('input:checked').count(), 0);
  assert.equal(await page.getByRole('button', { name: 'Select an agent to continue' }).isDisabled(), true);
  assert.equal(await page.getByRole('button', { name: 'Store', exact: true }).count(), 0);
  // Keyboard: the one enabled radio takes focus and space selects it.
  await page.getByRole('radio', { name: /^Sunday/ }).focus();
  await page.keyboard.press('Space');
  assert.equal(await page.getByRole('radio', { name: /^Sunday/ }).isChecked(), true);
  if (process.env.ONBOARDING_SCREENSHOT) await page.screenshot({ path: process.env.ONBOARDING_SCREENSHOT, fullPage: true });

  // A small window never scrolls sideways.
  await session.app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(900, 600));
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);

  // Choosing later is remembered across a real restart.
  await page.getByRole('button', { name: 'Choose later' }).click();
  await heading(page, 'Store');
  assert.deepEqual(JSON.parse(fs.readFileSync(file)), { version: 1, completed: true, selectedAgent: null });
  await session.app.close();
  page = await start();
  await heading(page, 'Store');

  // Sunday can be chosen later, from Settings; the choice survives a restart.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('button', { name: 'Change personal agent' }).click();
  await page.getByRole('radio', { name: /^Sunday/ }).check();
  await page.getByRole('button', { name: 'Continue with Sunday' }).click();
  await heading(page, 'Sunday');
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, 'hermes');
  await session.app.close();
  page = await start();
  await heading(page, 'Sunday');

  // A failed save keeps the chooser open with an error and the old choice on disk.
  await page.getByRole('button', { name: /^Settings/ }).click();
  await page.getByRole('button', { name: 'Change personal agent' }).click();
  fs.mkdirSync(`${file}.tmp`);
  await page.getByRole('button', { name: 'Use without an agent' }).click();
  await page.getByRole('alert').waitFor();
  await heading(page, 'Choose your personal agent');
  assert.equal(JSON.parse(fs.readFileSync(file)).selectedAgent, 'hermes');
  fs.rmdirSync(`${file}.tmp`);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await heading(page, 'Settings');

  // An earlier choice of an agent that is now "Coming soon" just shows the chooser again.
  fs.writeFileSync(file, JSON.stringify({ version: 1, completed: true, selectedAgent: 'openclaw' }));
  await page.reload();
  await heading(page, 'Choose your personal agent');
  assert.equal(await page.getByRole('alert').count(), 0);

  // Corrupt preferences are reported, kept, and replaced only by a new choice.
  fs.writeFileSync(file, '{broken');
  await page.reload();
  await page.getByRole('button', { name: 'Retry', exact: true }).waitFor();
  assert.equal(fs.readFileSync(file, 'utf8'), '{broken');
  await page.getByRole('button', { name: 'Choose again' }).click();
  await heading(page, 'Choose your personal agent');
  await page.getByRole('button', { name: 'Choose later' }).click();
  await heading(page, 'Store');
  assert.deepEqual(JSON.parse(fs.readFileSync(file)), { version: 1, completed: true, selectedAgent: null });
  assert.deepEqual(session.errors, []);
  console.log('Onboarding UI passed: Sunday only, coming soon, keyboard, small window, choose later, restart, change, failed save, retired choice, recovery.');
}

run().catch((error) => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (session) await session.app.close().catch(() => {});
  fs.rmSync(directory, { recursive: true, force: true });
});

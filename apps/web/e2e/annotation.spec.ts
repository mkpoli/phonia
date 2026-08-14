import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { openEditorWithFixture } from './helpers';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const wavFixture = path.join(root, 'tests/fixtures/audio/arctic_bdl_a0001.wav');
const textGridFixture = path.join(root, 'tests/fixtures/textgrids/arctic_bdl_a0001_long_utf8.TextGrid');
const screenshots = path.join(here, 'screenshots');

async function loadFixture(page: Page) {
  await openEditorWithFixture(page, wavFixture);
  // Loading the fixture journals two reversible steps: the audio import and the
  // empty annotation document attached when the recording opens. Both queue
  // behind the whole-signal analyses in the engine worker.
  await expect(page.getByTestId('tier-pane')).toHaveAttribute('data-undo-depth', '2', {
    timeout: 60_000
  });
}

function pane(page: Page) {
  return page.getByTestId('tier-pane');
}

function stateHash(page: Page) {
  return pane(page).getAttribute('data-state-hash');
}

function undoDepth(page: Page) {
  return pane(page)
    .getAttribute('data-undo-depth')
    .then((value) => Number(value));
}

async function playFor(page: Page, ms: number) {
  await page.keyboard.press('Space');
  await page.waitForTimeout(ms);
  await page.keyboard.press('Space');
}

test('keyboard-only annotation: tier, boundaries, labels, merge, undo x5, redo x5', async ({
  page
}) => {
  await loadFixture(page);

  // Create the tier from the keyboard: focus the button, press Enter.
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');
  await expect(page.getByTestId('interval')).toHaveCount(1);

  // Insert two boundaries at the playback cursor: play, pause, split (S).
  await pane(page).focus();
  await playFor(page, 700);
  const afterFirstPause = await stateHash(page);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  const afterSplit1 = await stateHash(page);
  expect(afterSplit1).not.toBe(afterFirstPause);

  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(3);
  const afterSplit2 = await stateHash(page);

  // Label the three intervals keyboard-only: digit focuses the tier, Enter
  // opens the editor, typed text commits with Enter, Tab advances.
  await page.keyboard.press('1');
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('ka');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(0)).toHaveAttribute('data-label', 'ka');

  await pane(page).focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('taː');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(1)).toHaveAttribute('data-label', 'taː');

  await pane(page).focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('na');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(2)).toHaveAttribute('data-label', 'na');
  const afterLabels = await stateHash(page);

  // Merge the active (last) interval into its neighbour with M.
  await pane(page).focus();
  await page.keyboard.press('m');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  const afterMerge = await stateHash(page);
  expect(await undoDepth(page)).toBe(9); // import, attach, tier, 2 splits, 3 labels, merge

  // Undo x5 (merge, three labels, second split) restores the split-only state.
  for (let i = 0; i < 5; i += 1) {
    await page.keyboard.press('Control+z');
  }
  await expect(pane(page)).toHaveAttribute('data-undo-depth', '4');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await expect(page.getByTestId('interval').nth(0)).toHaveAttribute('data-label', '');
  await expect.poll(() => stateHash(page)).toBe(afterSplit1);

  // Redo x5 reproduces the final state hash-identically.
  for (let i = 0; i < 5; i += 1) {
    await page.keyboard.press('Control+Shift+z');
  }
  await expect(pane(page)).toHaveAttribute('data-undo-depth', '9');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await expect.poll(() => stateHash(page)).toBe(afterMerge);

  // One more undo returns to the fully labeled three-interval state.
  await page.keyboard.press('Control+z');
  await expect(page.getByTestId('interval')).toHaveCount(3);
  await expect.poll(() => stateHash(page)).toBe(afterLabels);
  await expect(page.getByTestId('interval').nth(1)).toHaveAttribute('data-label', 'taː');
  const afterSplitsCheck = afterSplit2;
  expect(afterSplitsCheck).not.toBe(afterLabels);
});

test('arrow nudge moves the active boundary; Alt steps one frame', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await pane(page).focus();
  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);

  const xmaxBefore = Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax'));
  await page.keyboard.press('ArrowRight');
  await expect
    .poll(async () => Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax')))
    .toBeGreaterThan(xmaxBefore);

  // Alt+arrow nudges by exactly one sample frame (fixture is 16 kHz).
  const beforeAlt = Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax'));
  await page.keyboard.press('Alt+ArrowLeft');
  await expect
    .poll(async () => Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax')))
    .toBeLessThan(beforeAlt);
  const afterAlt = Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax'));
  expect(beforeAlt - afterAlt).toBeCloseTo(1 / 16000, 7);
});

test('boundary drag moves the boundary through the journal and undoes', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await pane(page).focus();
  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await expect(page.getByTestId('boundary-handle')).toHaveCount(1);

  const hashBefore = await stateHash(page);
  const handle = page.getByTestId('boundary-handle');
  const box = await handle.boundingBox();
  if (!box) throw new Error('boundary handle has no box');
  const startX = box.x + box.width / 2;
  const startY = box.y + box.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 160, startY, { steps: 8 });
  await page.mouse.up();

  const xmaxBefore = Number(await page.getByTestId('interval').nth(0).getAttribute('data-xmax'));
  expect(xmaxBefore).toBeGreaterThan(0);
  await expect.poll(() => stateHash(page)).not.toBe(hashBefore);

  // The drag is one journal entry: a single undo restores the pre-drag hash.
  await page.keyboard.press('Control+z');
  await expect.poll(() => stateHash(page)).toBe(hashBefore);
});

test('label search finds hits and navigates between them', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  await page.getByTestId('search-input').fill('il');
  await expect(page.getByTestId('search-count')).not.toHaveText('0');
  const first = await pane(page).getAttribute('data-active-index');
  await page.getByLabel('Next match').click();
  const second = await pane(page).getAttribute('data-active-index');
  expect(second).not.toBe(first);
  await page.getByLabel('Previous match').click();
  await expect(pane(page)).toHaveAttribute('data-active-index', String(first));
});

test('undoing a textgrid import repoints the pane instead of going blank', async ({ page }) => {
  await loadFixture(page);
  // Loading already journaled the audio import and an empty document (undo
  // depth 2, zero tiers); importing the TextGrid attaches a second document on
  // top of it rather than replacing it.
  await expect(pane(page)).toHaveAttribute('data-tier-count', '0');

  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');
  await expect(pane(page)).toHaveAttribute('data-undo-depth', '3');
  const importedHash = await stateHash(page);

  // Undo the import: the pane repoints to the pre-import document (empty,
  // not an error) instead of continuing to point at the now-detached one.
  await page.keyboard.press('Control+z');
  await expect(pane(page)).toHaveAttribute('data-undo-depth', '2');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '0');
  await expect(page.getByTestId('tier-empty')).toBeVisible();
  await expect(page.getByTestId('tier-status')).toHaveCount(0);

  // Redo reattaches the imported document and its tiers return.
  await page.keyboard.press('Control+Shift+z');
  await expect(pane(page)).toHaveAttribute('data-undo-depth', '3');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');
  await expect.poll(() => stateHash(page)).toBe(importedHash);
  await expect(
    page.getByTestId('interval').filter({ hasText: 'danger' }).first()
  ).toBeVisible();
});

test('textgrid import/export round trip and 4-tier screenshots in both themes', async ({
  page
}) => {
  await loadFixture(page);

  // Import the aligned TextGrid: words + phones interval tiers, events points.
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');
  await expect(page.getByTestId('tier-lane').first()).toHaveAttribute('data-tier-name', 'words');
  await expect(
    page.getByTestId('interval').filter({ hasText: 'danger' }).first()
  ).toBeVisible();
  // The point tier renders its points (zero-width anchors, so assert count).
  expect(await page.getByTestId('point').count()).toBeGreaterThan(0);

  // Export produces a TextGrid download carrying the same tier names.
  const downloadPromise = page.waitForEvent('download');
  await page.getByTestId('export-textgrid').click();
  const download = await downloadPromise;
  const stream = await download.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream) chunks.push(chunk as Buffer);
  const exported = Buffer.concat(chunks).toString('utf-8');
  expect(exported).toContain('"words"');
  expect(exported).toContain('"phones"');
  expect(exported).toContain('"events"');

  // A fourth tier from the keyboard, then themed screenshots.
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '4');

  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(screenshots, 'tiers-light.png'), fullPage: true });
  await page.getByLabel('Toggle theme').click();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(screenshots, 'tiers-dark.png'), fullPage: true });
});

test('measurement table harvests labelled intervals and exports CSV', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  // Open the measurement table through the command palette.
  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('measure labelled');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="measureIntervals"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await expect(cmd).toHaveAttribute('data-selected', 'true');
  await page.keyboard.press('Enter');

  // A row per labelled interval on the first (words) tier, measured from the
  // same engine queries the readout uses.
  await expect(page.getByTestId('measurement-table')).toBeVisible();
  const rows = page.getByTestId('measure-row');
  await expect(rows.first()).toBeVisible({ timeout: 30_000 });
  expect(await rows.count()).toBeGreaterThan(1);
  await expect(page.getByTestId('measure-grid')).toContainText('danger');

  // CSV export downloads a file whose header names the measurement columns.
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByTestId('measure-csv').click()
  ]);
  expect(download.suggestedFilename()).toMatch(/\.csv$/);
  const stream = await download.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream) chunks.push(chunk as Buffer);
  const csv = Buffer.concat(chunks).toString('utf-8');
  expect(csv.split('\n')[0]).toContain('F1 (Hz)');
  expect(csv).toContain('CoG (Hz)');
});

test('annotate by silences lays down a labelled speech/silence tier', async ({ page }) => {
  await loadFixture(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('annotate by silences');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="annotateBySilences"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await expect(cmd).toHaveAttribute('data-selected', 'true');
  await page.keyboard.press('Enter');

  // A new interval tier named "silences" appears with at least one sounding run.
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1', { timeout: 20_000 });
  await expect(page.getByTestId('tier-lane').first()).toHaveAttribute('data-tier-name', 'silences');
  await expect(
    page.getByTestId('interval').filter({ hasText: 'sounding' }).first()
  ).toBeVisible({ timeout: 20_000 });
});

test('the vowel chart plots F1-F2 for labelled intervals', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('vowel f1');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="vowelChart"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('vowel-chart-card')).toBeVisible();
  await expect(page.getByTestId('vowel-chart-canvas')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByTestId('vowel-chart-count')).toContainText('vowel');
});

test('the IPA pad inserts glyphs into the label editor without committing', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').click();
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');

  // Open the pad, then open the label editor on the interval.
  await page.getByTestId('ipa-toggle').click();
  await expect(page.getByTestId('ipa-pad')).toBeVisible();
  await page.getByTestId('interval').first().dblclick();
  const editor = page.getByTestId('label-editor');
  await expect(editor).toBeFocused();

  // Tapping a key inserts at the caret and never blurs (would commit) the field.
  await page.locator('[data-testid="ipa-key"][data-glyph="ʃ"]').click();
  await expect(editor).toBeFocused();
  await expect(editor).toHaveValue('ʃ');

  // Switching to the sinological tab keeps the editor open too.
  await page.getByTestId('ipa-tab-sino').click();
  await page.locator('[data-testid="ipa-key"][data-glyph="ɿ"]').first().click();
  await expect(editor).toHaveValue('ʃɿ');
});

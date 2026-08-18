import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { dismissTierName, openEditorWithFixture } from './helpers';

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
  await dismissTierName(page);

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
  // opens the editor, and committing with Enter steps to the next interval —
  // the loop is type-Enter-type without ever reaching for Tab.
  await page.keyboard.press('1');
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('ka');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(0)).toHaveAttribute('data-label', 'ka');

  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('taː');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(1)).toHaveAttribute('data-label', 'taː');

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

test('a cursor placed on the waveform feeds the annotation keys without a lane click', async ({
  page
}) => {
  await loadFixture(page);

  // Add the tier with a real toolbar click, then leave focus inside the
  // toolbar's search field — the annotation keys are dead from there.
  await page.getByTestId('add-interval-tier').click();
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');
  await expect(page.getByTestId('tier-status')).toHaveText('Added interval 1');
  await dismissTierName(page);
  await page.getByTestId('search-input').click();

  // Clicking the waveform places the cursor and hands the tier pane focus,
  // so the split key and the label loop act on the active tier at once.
  const wave = await page.getByTestId('waveform-canvas').boundingBox();
  await page.mouse.click(wave!.x + wave!.width * 0.4, wave!.y + wave!.height * 0.5);
  await expect(pane(page)).toBeFocused();
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);

  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('ka');
  await page.keyboard.press('Enter');
  // The split selects the interval the cursor landed in — the second one.
  await expect(page.getByTestId('interval').nth(1)).toHaveAttribute('data-label', 'ka');
  await expect(await undoDepth(page)).toBe(5); // import, attach, tier, split, label
});

test('the toolbar drives the whole annotation loop and names new tiers inline', async ({
  page
}) => {
  await loadFixture(page);

  // A new tier opens its own name field over the auto-name: type, Enter.
  await page.getByTestId('add-interval-tier').click();
  const nameField = page.getByTestId('tier-name-input');
  await expect(nameField).toBeFocused();
  await nameField.fill('words');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('tier-name-name')).toHaveText('words');
  // The pane takes the keyboard back, so S works with no extra click.
  await expect(pane(page)).toBeFocused();

  // Split twice from the toolbar with the cursor placed on the waveform.
  const wave = await page.getByTestId('waveform-canvas').boundingBox();
  await page.mouse.click(wave!.x + wave!.width * 0.3, wave!.y + wave!.height * 0.5);
  await page.getByTestId('split-at-cursor').click();
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await page.mouse.click(wave!.x + wave!.width * 0.6, wave!.y + wave!.height * 0.5);
  await page.getByTestId('split-at-cursor').click();
  await expect(page.getByTestId('interval')).toHaveCount(3);

  // Redo starts gated; undo and redo walk the journal one entry at a time.
  await expect(page.getByTestId('tier-redo')).toBeDisabled();
  const depth = await undoDepth(page);
  await page.getByTestId('tier-undo').click();
  await expect(page.getByTestId('interval')).toHaveCount(2);
  expect(await undoDepth(page)).toBe(depth - 1);
  await page.getByTestId('tier-redo').click();
  await expect(page.getByTestId('interval')).toHaveCount(3);

  // Label from the toolbar: the button opens the editor on the active item.
  await page.getByTestId('edit-label').click();
  await page.getByTestId('label-editor').fill('ta');
  await page.keyboard.press('Enter');
  await expect
    .poll(() =>
      page.getByTestId('interval').evaluateAll((els) =>
        els.some((el) => el.getAttribute('data-label') === 'ta')
      )
    )
    .toBe(true);

  // Merge from the toolbar folds the active interval back into its neighbour.
  await page.getByTestId('merge-active').click();
  await expect(page.getByTestId('interval')).toHaveCount(2);
});

test('splitting where a boundary already sits reports it in words, not an engine error', async ({
  page
}) => {
  await loadFixture(page);

  // A fresh tier spans the file and the cursor opens at 0, exactly on the
  // tier's edge — the split must answer with a sentence and change nothing.
  await page.getByTestId('add-interval-tier').click();
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');
  await dismissTierName(page);
  const depth = await undoDepth(page);
  await page.keyboard.press('s');
  await expect(page.getByTestId('tier-status')).toContainText(
    'The cursor sits on the edge of interval 1'
  );
  await expect(page.getByTestId('tier-status')).not.toContainText('annotation mutation failed');
  await expect(page.getByTestId('interval')).toHaveCount(1);
  expect(await undoDepth(page)).toBe(depth);

  // Interior case: split mid-file, then Tab parks the cursor exactly on the
  // boundary just created — the next split names that boundary in words.
  const wave = await page.getByTestId('waveform-canvas').boundingBox();
  await page.mouse.click(wave!.x + wave!.width * 0.4, wave!.y + wave!.height * 0.5);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await page.keyboard.press('Tab');
  await page.keyboard.press('s');
  await expect(page.getByTestId('tier-status')).toContainText('already has a boundary at');
  await expect(page.getByTestId('tier-status')).not.toContainText('annotation mutation failed');
  await expect(page.getByTestId('interval')).toHaveCount(2);
});

test('a successful add clears a stale error and auto-names never duplicate', async ({ page }) => {
  await loadFixture(page);

  // Leave an error on the status line: the cursor opens at the tier's edge.
  await page.getByTestId('add-interval-tier').click();
  await dismissTierName(page);
  await expect(page.getByTestId('tier-name-name')).toHaveText('interval 1');
  await page.keyboard.press('s');
  await expect(page.getByTestId('tier-status')).toContainText('The cursor sits on the edge');

  // The next add succeeds; the status line reports it instead of the old error.
  await page.getByTestId('add-interval-tier').click();
  await expect(page.getByTestId('tier-status')).toHaveText('Added interval 2');
  await dismissTierName(page);
  const names = page.getByTestId('tier-name-name');
  await expect(names).toHaveCount(2);
  await expect(names.nth(0)).toHaveText('interval 1');
  await expect(names.nth(1)).toHaveText('interval 2');

  // Remove the first tier, then add again: the count-based name would repeat
  // "interval 2", so the pane skips to the next free number.
  await page.getByTestId('remove-tier').first().click();
  await expect(page.getByTestId('tier-status')).toHaveText('Removed tier "interval 1"');
  await expect(names).toHaveCount(1);
  await expect(names.nth(0)).toHaveText('interval 2');
  await page.getByTestId('add-interval-tier').click();
  await expect(page.getByTestId('tier-status')).toHaveText('Added interval 3');
  await dismissTierName(page);
  await expect(names.nth(0)).toHaveText('interval 2');
  await expect(names.nth(1)).toHaveText('interval 3');
});

test('point tier: S adds a point at the cursor, undo removes it, label commits', async ({
  page
}) => {
  await loadFixture(page);

  await page.getByTestId('add-point-tier').focus();
  await page.keyboard.press('Enter');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');
  await expect(page.getByTestId('point')).toHaveCount(0);
  await dismissTierName(page);

  // Play to move the cursor, then S drops a point at it.
  await pane(page).focus();
  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('point')).toHaveCount(1);
  const afterPoint = await stateHash(page);

  // A second point at a later cursor.
  await playFor(page, 500);
  await page.keyboard.press('s');
  await expect(page.getByTestId('point')).toHaveCount(2);

  // Label the active point: Enter opens the editor, typed text commits.
  await pane(page).focus();
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('H*');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('point').nth(1)).toHaveAttribute('data-label', 'H*');

  // Undo the label and the second point, back to the single-point state.
  await pane(page).focus();
  await page.keyboard.press('Control+z');
  await page.keyboard.press('Control+z');
  await expect(page.getByTestId('point')).toHaveCount(1);
  await expect.poll(() => stateHash(page)).toBe(afterPoint);
});

test('point tier: M removes the active point, undo restores it', async ({ page }) => {
  await loadFixture(page);

  await page.getByTestId('add-point-tier').focus();
  await page.keyboard.press('Enter');
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1');
  await dismissTierName(page);

  // Drop two points at different cursor times.
  await pane(page).focus();
  await playFor(page, 600);
  await page.keyboard.press('s');
  await playFor(page, 500);
  await page.keyboard.press('s');
  await expect(page.getByTestId('point')).toHaveCount(2);
  const afterTwo = await stateHash(page);

  // M removes the active point.
  await pane(page).focus();
  await page.keyboard.press('m');
  await expect(page.getByTestId('point')).toHaveCount(1);

  // Undo brings it back, hash-identically.
  await page.keyboard.press('Control+z');
  await expect(page.getByTestId('point')).toHaveCount(2);
  await expect.poll(() => stateHash(page)).toBe(afterTwo);
});

test('arrow nudge moves the active boundary; Alt steps one frame', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await dismissTierName(page);
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

test('arrow nudge stops before collapsing the neighboring interval', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').click();
  await dismissTierName(page);

  // Leave less than one tier-lane pixel to the right of the boundary. A full
  // ArrowRight step must stay inside that remaining interval.
  const wave = await page.getByTestId('waveform-canvas').boundingBox();
  if (!wave) throw new Error('waveform has no box');
  await page.mouse.click(wave.x + wave.width - 0.5, wave.y + wave.height / 2);
  await pane(page).focus();
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);

  const right = page.getByTestId('interval').nth(1);
  const before = Number(await right.getAttribute('data-xmin'));
  const end = Number(await right.getAttribute('data-xmax'));
  await page.keyboard.press('ArrowRight');

  await expect
    .poll(async () => Number(await right.getAttribute('data-xmin')))
    .toBeGreaterThan(before);
  const after = Number(await right.getAttribute('data-xmin'));
  expect(after).toBeLessThan(end);
  expect((await page.getByTestId('tier-status').allTextContents()).join(' ')).not.toContain(
    'annotation mutation failed'
  );
});

test('boundary drag moves the boundary through the journal and undoes', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await dismissTierName(page);
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
  // The tier controls occupy the lane's upper-left band. Grab the exposed
  // lower part of a boundary that happens to pass behind those controls.
  const startY = box.y + box.height * 0.8;
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

test('point drag moves the point through the journal and undoes', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('add-point-tier').focus();
  await page.keyboard.press('Enter');

  // Click seats the cursor mid-lane so the points land clear of the left-edge
  // tier chip; two points give the dragged one a neighbour to stay ordered
  // against.
  await pane(page).click();
  await playFor(page, 500);
  await page.keyboard.press('s');
  await playFor(page, 400);
  await page.keyboard.press('s');
  await expect(page.getByTestId('point')).toHaveCount(2);

  const hashBefore = await stateHash(page);
  const first = page.getByTestId('point').nth(0);
  const timeBefore = Number(await first.getAttribute('data-time'));
  const box = await first.boundingBox();
  if (!box) throw new Error('point has no box');
  const startX = box.x + box.width / 2;
  const startY = box.y + box.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX - 60, startY, { steps: 8 });
  await page.mouse.up();

  // The point moved earlier in time; the move is a single journal entry.
  await expect.poll(() => stateHash(page)).not.toBe(hashBefore);
  const timeAfter = Number(await page.getByTestId('point').nth(0).getAttribute('data-time'));
  expect(timeAfter).toBeLessThan(timeBefore);

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

test('replace all rewrites every matching label', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  await page.getByTestId('search-input').fill('il');
  await expect(page.getByTestId('search-count')).not.toHaveText('0');

  await page.getByTestId('replace-input').fill('QZ');
  await page.getByTestId('replace-all').click();

  // The matched substring is gone, so the same query now finds nothing.
  await expect(page.getByTestId('search-count')).toHaveText('0');
  // The replacement text is now present in the tier.
  await page.getByTestId('search-input').fill('QZ');
  await expect(page.getByTestId('search-count')).not.toHaveText('0');
});

test('add on all tiers drops a boundary and a point on every tier at the cursor', async ({
  page
}) => {
  await loadFixture(page);

  // Two interval tiers and one point tier.
  await page.getByTestId('add-interval-tier').click();
  await page.getByTestId('add-interval-tier').click();
  await page.getByTestId('add-point-tier').click();
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  const intervalLanes = page.locator('[data-testid="tier-lane"][data-tier-kind="interval"]');
  const pointLanes = page.locator('[data-testid="tier-lane"][data-tier-kind="point"]');
  await expect(intervalLanes).toHaveCount(2);
  await expect(pointLanes).toHaveCount(1);
  // Each interval tier starts as one full-span interval; the point tier is empty.
  for (const lane of await intervalLanes.all()) {
    await expect(lane.getByTestId('interval')).toHaveCount(1);
  }
  await expect(pointLanes.first().getByTestId('point')).toHaveCount(0);

  // Seat the cursor mid-signal by playing then pausing.
  await pane(page).focus();
  await playFor(page, 700);

  await page.getByTestId('add-on-all-tiers').click();

  // Every interval tier gained a boundary (one interval → two) and the point
  // tier gained a point.
  for (const lane of await intervalLanes.all()) {
    await expect(lane.getByTestId('interval')).toHaveCount(2);
  }
  await expect(pointLanes.first().getByTestId('point')).toHaveCount(1);
  await expect(page.getByTestId('tier-status')).toHaveText('Added on 3 tiers');
});

test('snap boundary to nearest zero crossing lands it on a crossing', async ({ page }) => {
  await loadFixture(page);

  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval')).toHaveCount(1);
  await dismissTierName(page);

  // Split at a mid-signal cursor to make an internal boundary off any crossing.
  await pane(page).focus();
  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);

  // Snap that boundary to the nearest zero crossing via the command palette.
  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('zero crossing');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="snapBoundaryZero"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('tier-status')).toHaveText('Snapped to zero crossing');

  // The boundary now sits on a zero crossing: re-querying the nearest crossing
  // of the boundary time returns the same time (snap is idempotent).
  await expect
    .poll(
      async () => {
        const t = Number(await page.getByTestId('interval').first().getAttribute('data-xmax'));
        if (!Number.isFinite(t)) return false;
        const again = await page.evaluate(async (time) => {
          const hook = (
            globalThis as unknown as { __phonia?: { client: any; audioId: bigint | null } }
          ).__phonia;
          if (!hook || hook.audioId === null) throw new Error('no client hook');
          return (await hook.client.nearestZeroCrossing(hook.audioId, time)) as number;
        }, t);
        return Math.abs(again - t) < 1e-6;
      },
      { timeout: 15_000 }
    )
    .toBe(true);
});

test('copy annotation table writes every tier row to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);
  await expect(pane(page)).toHaveAttribute('data-tier-count', '3');

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('annotation table');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyAnnotationTable"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('tier-status')).toContainText('Copied');

  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('tier,kind,label,tmin,tmax')).toBe(true);
  const lines = csv.trim().split('\n');
  // The three-tier fixture carries many labels, so the dump has plenty of rows.
  expect(lines.length).toBeGreaterThan(3);
  // Each data row names a valid kind and carries the five columns.
  const cols = lines[1].split(',');
  expect(cols.length).toBeGreaterThanOrEqual(5);
  expect(['interval', 'point']).toContain(cols[1]);
});

test('reorder tier moves it up or down in the stack', async ({ page }) => {
  await openEditorWithFixture(page, wavFixture);
  const lanes = page.getByTestId('tier-lane');
  await page.getByTestId('add-interval-tier').click();
  await expect(lanes).toHaveCount(1);
  await page.getByTestId('add-interval-tier').click();
  await expect(lanes).toHaveCount(2);
  const order = async () =>
    Promise.all((await lanes.all()).map((lane) => lane.getAttribute('data-tier-name')));
  expect(await order()).toEqual(['interval 1', 'interval 2']);

  // The topmost tier's up control is disabled; moving it down swaps the pair.
  await expect(page.getByTestId('move-tier-up').first()).toBeDisabled();
  await page.getByTestId('move-tier-down').first().click();
  await expect.poll(order).toEqual(['interval 2', 'interval 1']);
});

test('duplicate tier copies it below with a " copy" name', async ({ page }) => {
  await openEditorWithFixture(page, wavFixture);
  const lanes = page.getByTestId('tier-lane');
  await page.getByTestId('add-interval-tier').click();
  await expect(lanes).toHaveCount(1);
  await dismissTierName(page);

  await page.getByTestId('duplicate-tier').first().click();
  await expect(lanes).toHaveCount(2);
  const names = async () =>
    Promise.all((await lanes.all()).map((lane) => lane.getAttribute('data-tier-name')));
  expect(await names()).toEqual(['interval 1', 'interval 1 copy']);
});

test('extract labelled intervals adds one recording per interval', async ({ page }) => {
  await loadFixture(page);
  await page.getByTestId('textgrid-input').setInputFiles(textGridFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('measure');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="measureIntervals"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('measure-grid')).toBeVisible({ timeout: 20_000 });
  const rowCount = await page.getByTestId('measure-row').count();
  expect(rowCount).toBeGreaterThan(0);

  await page.getByTestId('measure-extract').click();
  await expect(page.getByTestId('editor-toast')).toContainText('Extracted', { timeout: 40_000 });

  // The library gained one recording per labelled interval, and the original.
  await page.getByTestId('measure-close').click();
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(rowCount + 1);
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
  await dismissTierName(page);

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

test('annotate by voicing lays down a V/U tier', async ({ page }) => {
  await loadFixture(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('annotate by voicing');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="annotateByVoicing"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  // A new interval tier named "voicing" appears with at least one voiced (V) run.
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1', { timeout: 20_000 });
  await expect(page.getByTestId('tier-lane').first()).toHaveAttribute('data-tier-name', 'voicing');
  await expect(
    page.getByTestId('interval').filter({ hasText: 'V' }).first()
  ).toBeVisible({ timeout: 20_000 });
});

test('extract selection with tiers crops the labelled tier into the new recording', async ({
  page
}) => {
  await loadFixture(page);

  // A labelled interval tier: split once, name the first interval.
  await page.getByTestId('add-interval-tier').focus();
  await page.keyboard.press('Enter');
  await dismissTierName(page);
  await pane(page).focus();
  await playFor(page, 700);
  await page.keyboard.press('s');
  await expect(page.getByTestId('interval')).toHaveCount(2);
  await page.keyboard.press('1');
  await page.keyboard.press('Enter');
  await page.getByTestId('label-editor').fill('aa');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('interval').nth(0)).toHaveAttribute('data-label', 'aa');

  // Drag a time–frequency box that spans the labelled interval.
  const canvas = page.getByTestId('spectrogram-canvas');
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.05, box.y + box.height * 0.4);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.9, box.y + box.height * 0.6, { steps: 6 });
  await page.mouse.up();
  await expect(page.getByTestId('readout-bar')).toBeVisible();

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('extract selection with tiers');
  const cmd = page.locator(
    '[data-testid="command-item"][data-command-id="extractSelectionWithTiers"]'
  );
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  // The new extract opens with its tier carried across, still labelled.
  await expect(page.getByTestId('recording-switcher-name')).toContainText('[', { timeout: 20_000 });
  await expect(pane(page)).toHaveAttribute('data-tier-count', '1', { timeout: 20_000 });
  await expect(
    page.getByTestId('interval').filter({ hasText: 'aa' }).first()
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

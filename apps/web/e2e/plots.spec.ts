import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { openEditorWithFixture } from './helpers';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const wavFixture = path.join(root, 'tests/fixtures/audio/arctic_bdl_a0001.wav');

/** Opens the editor on the fixture, then switches to the Plots workspace. */
async function openPlots(page: Page) {
  await openEditorWithFixture(page, wavFixture);
  await page.getByRole('button', { name: 'Plots' }).click();
  await expect(page.getByTestId('plots-view')).toBeVisible();
}

/** Adds one layer of `kind` from the toolbar menu. */
async function addLayer(page: Page, kind: string) {
  await page.getByTestId('plots-add').click();
  await page.getByTestId(`plots-add-${kind}`).click();
}

/** The rendered figure SVG for the first object, once it has been built. */
async function objectSvg(page: Page): Promise<string> {
  const svg = page.locator('[data-testid="plots-canvas"] svg').first();
  await expect(svg).toBeVisible({ timeout: 30_000 });
  return svg.evaluate((el) => el.outerHTML);
}

test('plots: adding a layer renders one figure object', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');

  await expect(page.getByTestId('plots-obj')).toHaveCount(1);
  const svg = await objectSvg(page);
  expect(svg).toContain('<svg');
  // The waveform figure carries its value-axis title.
  expect(svg).toContain('Amplitude');
});

test('plots: arrow keys nudge the selected object and one undo reverts the run', async ({
  page
}) => {
  await openPlots(page);
  await addLayer(page, 'waveform');

  const frameX = page.getByTestId('plots-frame-x');
  const startX = Number(await frameX.inputValue());

  // A new object is auto-selected and the canvas is focused, so the arrow keys
  // nudge it straight away: one pixel a press, ten with Shift held.
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(frameX).toHaveValue(String(startX + 3));
  await page.keyboard.press('Shift+ArrowRight');
  await expect(frameX).toHaveValue(String(startX + 13));

  // The whole uninterrupted run collapses into a single undo step.
  await page.getByTestId('plots-undo').click();
  await expect(frameX).toHaveValue(String(startX));
});

test('plots: toggling grid lines re-renders the figure', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  const before = await objectSvg(page);

  // Grid is an artboard-level control, shown when no object is selected: click
  // empty canvas to deselect, then turn the interior grid lines off.
  await page.getByTestId('plots-canvas').click({ position: { x: 40, y: 720 } });
  await page.getByTestId('plots-grid').uncheck();

  await expect
    .poll(async () => (await objectSvg(page)) !== before, { timeout: 15_000 })
    .toBe(true);
});

test('plots: Delete removes the selected object', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await expect(page.getByTestId('plots-obj')).toHaveCount(1);

  await page.keyboard.press('Delete');
  await expect(page.getByTestId('plots-obj')).toHaveCount(0);
});

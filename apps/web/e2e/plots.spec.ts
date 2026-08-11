import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
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

/** Triggers a download from `testId` and returns its name and bytes. */
async function download(page: Page, testId: string): Promise<{ name: string; buffer: Buffer }> {
  const [file] = await Promise.all([
    page.waitForEvent('download'),
    page.getByTestId(testId).click()
  ]);
  const onDisk = await file.path();
  return { name: file.suggestedFilename(), buffer: fs.readFileSync(onDisk) };
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

test('plots: Escape deselects the active object', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  // A selected object shows its eight resize handles.
  await expect(page.locator('[data-testid^="plots-handle-"]')).toHaveCount(8);

  await page.keyboard.press('Escape');
  await expect(page.locator('[data-testid^="plots-handle-"]')).toHaveCount(0);
});

test('plots: the figure survives switching to Analyse and back', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await expect(page.getByTestId('plots-obj')).toHaveCount(1);

  await page.getByRole('button', { name: 'Analyse' }).click();
  await expect(page.getByTestId('editor')).toBeVisible();
  await page.getByRole('button', { name: 'Plots' }).click();

  // The editor is kept mounted, so the composed figure is still there.
  await expect(page.getByTestId('plots-obj')).toHaveCount(1);
});

test('plots: a selected object exposes eight resize handles', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  // Four corners plus four mid-edges, so a single dimension can be resized.
  await expect(page.locator('[data-testid^="plots-handle-"]')).toHaveCount(8);
});

test('plots: dragging the paper corner resizes the artboard', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');

  const artboard = page.getByTestId('plots-artboard');
  const w0 = (await artboard.boundingBox())!.width;
  const grip = await page.getByTestId('plots-paper-handle-se').boundingBox();
  const cx = grip!.x + grip!.width / 2;
  const cy = grip!.y + grip!.height / 2;
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 160, cy + 120, { steps: 8 });
  await page.mouse.up();

  const w1 = (await artboard.boundingBox())!.width;
  expect(w1).toBeGreaterThan(w0 + 100);
  // One undo restores the pre-drag paper size.
  await page.getByTestId('plots-undo').click();
  expect((await artboard.boundingBox())!.width).toBeLessThan(w1 - 100);
});

test('plots: align buttons snap the object to the paper edges', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');

  const frameX = page.getByTestId('plots-frame-x');
  // Align left pins the box to x = 0.
  await page.getByTestId('plots-align-left').click();
  await expect(frameX).toHaveValue('0');

  // Centre horizontally puts x at (paperW - w) / 2; for a fresh 760-wide paper
  // and a 420-wide object that is 170.
  await page.getByTestId('plots-align-center').click();
  await expect(frameX).toHaveValue('170');

  // One undo reverts the last align.
  await page.getByTestId('plots-undo').click();
  await expect(frameX).toHaveValue('0');
});

test('plots: a text label renders on the canvas and into the export', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'text');

  // The new text object shows its default label and is auto-selected.
  const label = page.locator('[data-testid="plots-obj"][data-kind="text"] .obj-text');
  await expect(label).toHaveText('Label');

  // Editing the content updates the canvas live.
  await page.getByTestId('plots-text-input').fill('Formant onset');
  await expect(label).toHaveText('Formant onset');

  // A text object has no time window, so those controls are hidden.
  await expect(page.getByTestId('plots-window-start')).toHaveCount(0);

  // The label composes into the exported SVG as real text.
  await page.getByTestId('plots-export').click();
  const { name, buffer } = await download(page, 'plots-export-svg');
  expect(name).toMatch(/\.svg$/);
  expect(buffer.toString('utf8')).toContain('Formant onset');
});

test('plots: a new object adopts the Analyse time selection', async ({ page }) => {
  await openEditorWithFixture(page, wavFixture);

  // Drag a box on the spectrogram to set a time selection in Analyse.
  const canvas = page.getByTestId('spectrogram-canvas');
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.4);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.7, box.y + box.height * 0.6, { steps: 6 });
  await page.mouse.up();

  const bar = page.getByTestId('readout-bar');
  await expect(bar).toBeVisible();
  const t0 = Number(await bar.getAttribute('data-t0'));
  const t1 = Number(await bar.getAttribute('data-t1'));
  expect(t1).toBeGreaterThan(t0);

  // Switch to Plots and add a layer; it opens scoped to that span.
  await page.getByRole('button', { name: 'Plots' }).click();
  await expect(page.getByTestId('plots-view')).toBeVisible();
  await addLayer(page, 'waveform');

  const start = Number(await page.getByTestId('plots-window-start').inputValue());
  const end = Number(await page.getByTestId('plots-window-end').inputValue());
  expect(start).toBeCloseTo(t0, 2);
  expect(end).toBeCloseTo(t1, 2);

  // 'Full recording' clears the window back to the whole signal.
  await page.getByTestId('plots-full-recording').click();
  await expect(page.getByTestId('plots-window-start')).toHaveValue('');
  await expect(page.getByTestId('plots-window-end')).toHaveValue('');
});

test('plots: bracket keys restack the selected object', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await addLayer(page, 'pitch');

  // The layers list is front-most first; the just-added Pitch sits on top.
  const names = page.getByTestId('plots-layer-item');
  await expect(names.first()).toHaveText('Pitch');

  // '[' sends the selection back one step.
  await page.keyboard.press('[');
  await expect(names.first()).toHaveText('Waveform');

  // Shift+']' brings it all the way back to the front.
  await page.keyboard.press('Shift+]');
  await expect(names.first()).toHaveText('Pitch');
});

test('plots: scrolling the wheel pans the artboard', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');

  const artboard = page.getByTestId('plots-artboard');
  const y0 = (await artboard.boundingBox())!.y;

  // A plain wheel (no modifier) scrolls the view; down moves the paper up.
  const canvas = await page.getByTestId('plots-canvas').boundingBox();
  await page.mouse.move(canvas!.x + canvas!.width / 2, canvas!.y + canvas!.height / 2);
  await page.mouse.wheel(0, 200);

  const y1 = (await artboard.boundingBox())!.y;
  expect(y1).toBeLessThan(y0 - 100);
});

test('plots: fit paper to content shrinks the artboard to the objects', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  const artboard = page.getByTestId('plots-artboard');
  const before = (await artboard.boundingBox())!.width;

  // Fit lives on the artboard panel, shown when nothing is selected.
  await page.getByTestId('plots-canvas').click({ position: { x: 40, y: 720 } });
  await page.getByTestId('plots-fit-paper').click();

  const after = (await artboard.boundingBox())!.width;
  expect(after).toBeLessThan(before);
  await page.getByTestId('plots-undo').click();
  expect((await artboard.boundingBox())!.width).toBeGreaterThan(after);
});

test('plots: exports the composed figure as SVG', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await objectSvg(page); // wait for the object figure to build before exporting

  await page.getByTestId('plots-export').click();
  const { name, buffer } = await download(page, 'plots-export-svg');
  // The filename derives from the project/title; only the extension is fixed.
  expect(name).toMatch(/\.svg$/);
  const svg = buffer.toString('utf8');
  expect(svg.startsWith('<svg')).toBe(true);
  // The composed document nests the waveform object, axis title and all.
  expect(svg).toContain('Amplitude');
});

test('plots: exports the composed figure as PNG', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await objectSvg(page);

  await page.getByTestId('plots-export').click();
  const { name, buffer } = await download(page, 'plots-export-png');
  expect(name).toMatch(/\.png$/);
  // PNG magic number: the rasteriser produced a real image, not an empty blob.
  expect(buffer.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
});

test('plots: the export filename follows the figure title', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  await objectSvg(page);

  // The title lives on the artboard panel, shown when nothing is selected.
  await page.getByTestId('plots-canvas').click({ position: { x: 40, y: 720 } });
  await page.getByTestId('plots-title-input').fill('Danger Trail F2');
  await page.getByTestId('plots-export').click();
  const { name } = await download(page, 'plots-export-svg');
  expect(name).toBe('danger-trail-f2.svg');
});

test('plots: setting an axis colour recolours the rendered figure', async ({ page }) => {
  await openPlots(page);
  await addLayer(page, 'waveform');
  const before = await objectSvg(page);

  // Axis colour is an artboard control: deselect, then pick a preset far from
  // the theme default so the axis strokes carry the chosen colour.
  await page.getByTestId('plots-canvas').click({ position: { x: 40, y: 720 } });
  await page.getByTestId('plots-axis-color').click();
  await page.getByRole('button', { name: '#c80000' }).click();

  await expect
    .poll(async () => (await objectSvg(page)).includes('c80000'), { timeout: 15_000 })
    .toBe(true);
  expect(await objectSvg(page)).not.toBe(before);
});

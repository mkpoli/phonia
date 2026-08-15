import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { openEditorWithFixture } from './helpers';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const vowelFixture = path.join(root, 'tests/fixtures/audio/synth_vowel_a.wav');
const stereoFixture = path.join(root, 'tests/fixtures/audio/synth_stereo.wav');
const screenshots = path.join(here, 'screenshots');

interface BoxCoords {
  t0: number;
  t1: number;
  f0: number;
  f1: number;
}

/** Drags a time–frequency box across the spectrogram and returns its signal coordinates. */
async function dragSpectrogramBox(page: Page): Promise<BoxCoords> {
  const canvas = page.getByTestId('spectrogram-canvas');
  const box = await canvas.boundingBox();
  if (!box) throw new Error('spectrogram canvas has no box');
  const x0 = box.x + box.width * 0.3;
  const x1 = box.x + box.width * 0.7;
  const y0 = box.y + box.height * 0.3;
  const y1 = box.y + box.height * 0.7;
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move((x0 + x1) / 2, (y0 + y1) / 2, { steps: 4 });
  await page.mouse.move(x1, y1, { steps: 4 });
  await page.mouse.up();

  const bar = page.getByTestId('readout-bar');
  await expect(bar).toBeVisible();
  // The readout is an async engine query; wait for it to settle before callers
  // read any measured value.
  await expect
    .poll(async () => {
      const value = await page.getByTestId('readout-band-energy').getAttribute('data-value');
      return value !== null && value !== '' && Number.isFinite(Number(value));
    }, { timeout: 30_000 })
    .toBe(true);
  const read = async (attr: string) => Number(await bar.getAttribute(attr));
  return { t0: await read('data-t0'), t1: await read('data-t1'), f0: await read('data-f0'), f1: await read('data-f1') };
}

/** The spectrogram-pane selection box (both panes render a box for a shared selection). */
function spectrogramBox(page: Page) {
  return page.getByTestId('selection-layer-box').getByTestId('selection-box');
}

test('spectrogram box selection: readout values are present and finite', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  const coords = await dragSpectrogramBox(page);

  expect(coords.t1).toBeGreaterThan(coords.t0);
  expect(coords.f1).toBeGreaterThan(coords.f0);

  const band = page.getByTestId('readout-band-energy');
  await expect(band).toBeVisible();
  const bandValue = Number(await band.getAttribute('data-value'));
  expect(Number.isFinite(bandValue)).toBe(true);

  const f0Mean = Number(await page.getByTestId('readout-f0-mean').getAttribute('data-value'));
  expect(Number.isFinite(f0Mean)).toBe(true);
  expect(f0Mean).toBeGreaterThan(0);

  // Spectral centre of gravity is surfaced from the core's spectral moments and
  // is a positive frequency over a voiced vowel.
  const cog = Number(await page.getByTestId('readout-cog').getAttribute('data-value'));
  expect(Number.isFinite(cog)).toBe(true);
  expect(cog).toBeGreaterThan(0);

  // RMS and absolute peak over the selection: both finite, positive, and bounded
  // by full scale, with the peak never below the RMS.
  const rms = Number(await page.getByTestId('readout-rms').getAttribute('data-value'));
  const peak = Number(await page.getByTestId('readout-peak').getAttribute('data-value'));
  expect(Number.isFinite(rms)).toBe(true);
  expect(Number.isFinite(peak)).toBe(true);
  expect(rms).toBeGreaterThan(0);
  expect(peak).toBeGreaterThanOrEqual(rms);
  expect(peak).toBeLessThanOrEqual(1.0001);

  // F0 and intensity spread over the selection: both finite and non-negative.
  const f0Sd = Number(await page.getByTestId('readout-f0-sd').getAttribute('data-value'));
  const intSd = Number(await page.getByTestId('readout-intensity-sd').getAttribute('data-value'));
  expect(Number.isFinite(f0Sd)).toBe(true);
  expect(Number.isFinite(intSd)).toBe(true);
  expect(f0Sd).toBeGreaterThanOrEqual(0);
  expect(intSd).toBeGreaterThanOrEqual(0);

  // The 5–95% F0 quantiles bound the contour: both positive, the 5th below the
  // 95th.
  const quantile = page.getByTestId('readout-f0-quantile');
  const p5 = Number(await quantile.getAttribute('data-value'));
  expect(Number.isFinite(p5)).toBe(true);
  expect(p5).toBeGreaterThan(0);
  const quantileText = (await quantile.locator('.v').textContent()) ?? '';
  const bounds = [...quantileText.matchAll(/[\d.]+/g)].map((m) => Number(m[0]));
  expect(bounds.length).toBeGreaterThanOrEqual(2);
  expect(bounds[0]).toBeLessThanOrEqual(bounds[1]);

  await expect(page.getByTestId('readout-duration')).toContainText('s');
});

test('the formant ceiling sweep tabulates F1-F4 at several LPC ceilings', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('formant ceiling sweep');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="formantCeilingSweep"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('formant-sweep-card')).toBeVisible();
  const rows = page.getByTestId('formant-sweep-row');
  await expect(rows).toHaveCount(4, { timeout: 20_000 });
  await expect(rows.first()).toContainText('4500');
  // The steadiest ceiling is flagged.
  await expect(page.getByTestId('formant-sweep-grid')).toContainText('steadiest');
});

test('LTAS shows an averaged spectrum over the selection', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('ltas');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="ltas"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('spectrum-card')).toContainText('LTAS');
  await expect(page.getByTestId('spectrum-canvas')).toBeVisible({ timeout: 15_000 });
});

test('the spectrum card plots the selection slice with a hover readout', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.getByTestId('selection-spectrum').click();
  await expect(page.getByTestId('spectrum-card')).toBeVisible();
  const canvas = page.getByTestId('spectrum-canvas');
  await expect(canvas).toBeVisible({ timeout: 15_000 });

  // Hovering the plot reads out the frequency and dB under the cursor.
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.5);
  await expect(page.getByTestId('spectrum-readout')).toContainText('Hz');
  await expect(page.getByTestId('spectrum-readout')).toContainText('dB');

  // The LPC-smoothed envelope overlays the slice and can be toggled off and on.
  const lpc = page.getByTestId('spectrum-lpc-toggle');
  await expect(lpc).toBeVisible();
  await expect(lpc).toHaveAttribute('aria-pressed', 'true');
  await lpc.click();
  await expect(lpc).toHaveAttribute('aria-pressed', 'false');
  await lpc.click();
  await expect(lpc).toHaveAttribute('aria-pressed', 'true');
});

test('the cepstrum card plots quefrency with a per-quefrency readout', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('cepstrum');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="cepstrum"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('spectrum-card')).toContainText('Cepstrum');
  const canvas = page.getByTestId('spectrum-canvas');
  await expect(canvas).toBeVisible({ timeout: 15_000 });

  // Hovering the plot reads quefrency in milliseconds, not frequency.
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.5);
  await expect(page.getByTestId('spectrum-readout')).toContainText('ms');
});

test('the pitch unit selector converts the F0 readout to semitones', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  const f0 = page.getByTestId('readout-f0-mean');
  await expect(f0).toContainText('Hz');

  // Switching the pitch unit reformats every F0 statistic in the readout.
  await page.getByTestId('pitch-unit').selectOption('semitones');
  await expect(f0).toContainText('st');
  await expect(page.getByTestId('readout-f0-range')).toContainText('st');
});

test('extract selection copies the span into a new library recording and opens it', async ({
  page
}) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.getByTestId('selection-extract').click();

  // The extract becomes the active recording, named with its time span in seconds.
  const name = page.getByTestId('recording-switcher-name');
  await expect(name).toContainText('[', { timeout: 20_000 });
  await expect(name).toContainText('s]');

  // Both takes — the original and the extract — now live in the library.
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('filter box selection stores a band-passed copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  // The Filter action appears only for a box selection (it needs a frequency band).
  await page.getByTestId('selection-filter').click();

  const name = page.getByTestId('recording-switcher-name');
  await expect(name).toContainText('Hz]', { timeout: 20_000 });

  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('reverse selection stores a time-reversed copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.getByTestId('selection-reverse').click();

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[reversed]', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('scale intensity command stores a 70 dB copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('scale intensity');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="scaleSelection"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[70 dB]', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('scale peak command stores a peak-normalized copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('scale peak');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="scalePeakSelection"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[peak 0.99]', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('snap selection to zero crossings lands the edges on crossings', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('snap');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="snapSelectionZero"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  const box = spectrogramBox(page);
  await expect(box).toBeVisible();

  // The snapped edge sits on a zero crossing: re-querying the nearest crossing
  // of the snapped start returns the same time (the snap is idempotent).
  await expect
    .poll(async () => {
      const t0 = Number(await box.getAttribute('data-sel-t0'));
      if (!Number.isFinite(t0)) return false;
      const again = await page.evaluate(async (t) => {
        const hook = (globalThis as unknown as { __phonia?: { client: any; audioId: bigint | null } })
          .__phonia;
        if (!hook || hook.audioId === null) throw new Error('no client hook');
        return (await hook.client.nearestZeroCrossing(hook.audioId, t)) as number;
      }, t0);
      return Math.abs(again - t0) < 1e-6;
    }, { timeout: 15_000 })
    .toBe(true);
});

test('copy pitch contour writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('pitch contour');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyPitchContour"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('Pitch contour copied', {
    timeout: 20_000
  });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('time_s,f0_hz')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const [t, f0] = lines[1].split(',').map(Number);
  expect(Number.isFinite(t)).toBe(true);
  expect(f0).toBeGreaterThan(0);
});

test('copy intensity contour writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('intensity contour');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyIntensityContour"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('Intensity contour copied', {
    timeout: 20_000
  });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('time_s,intensity_db')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const [t, db] = lines[1].split(',').map(Number);
  expect(Number.isFinite(t)).toBe(true);
  expect(Number.isFinite(db)).toBe(true);
});

test('resample to 16 kHz stores a downsampled copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('resample');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="resample16k"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[16 kHz]', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('copy formant contour writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('formant contour');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyFormantContour"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('Formant contour copied', {
    timeout: 20_000
  });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('time_s,f1_hz')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const cells = lines[1].split(',');
  expect(Number.isFinite(Number(cells[0]))).toBe(true);
  expect(Number(cells[1])).toBeGreaterThan(0);
});

test('intensity extrema show the max and min over the selection', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  const max = page.getByTestId('intensity-max');
  const min = page.getByTestId('intensity-min');
  await expect(max).toContainText('dB', { timeout: 20_000 });
  await expect(max).toContainText('s');
  await expect(min).toContainText('dB');

  const parse = async (loc: ReturnType<typeof page.getByTestId>) =>
    Number((await loc.textContent())!.match(/(-?\d+\.\d+)\s*dB/)![1]);
  const maxDb = await parse(max);
  const minDb = await parse(min);
  expect(maxDb).toBeGreaterThanOrEqual(minDb);
});

test('copy spectrum writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('copy spectrum');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copySpectrum"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('Spectrum copied', {
    timeout: 20_000
  });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('freq_hz,db')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const [f, d] = lines[1].split(',').map(Number);
  expect(f).toBeGreaterThanOrEqual(0);
  expect(Number.isFinite(d)).toBe(true);
});

test('copy harmonicity contour writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('harmonicity');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyHarmonicityContour"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('Harmonicity copied', {
    timeout: 20_000
  });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('time_s,hnr_db')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const [t, hnr] = lines[1].split(',').map(Number);
  expect(Number.isFinite(t)).toBe(true);
  expect(Number.isFinite(hnr)).toBe(true);
});

test('harmonicity overlay: the layer toggles on', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  const eye = page.getByTestId('toggle-harmonicity');
  await expect(eye).toHaveAttribute('aria-pressed', 'false');
  await eye.click();
  await expect(eye).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('inspector-harmonicity')).not.toHaveClass(/\boff\b/);
});

test('harmonicity extrema: min/max HNR over the selection appear when the layer is on', async ({
  page
}) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);
  await page.getByTestId('toggle-harmonicity').click();

  const extrema = page.getByTestId('harmonicity-extrema');
  await expect(extrema).toBeVisible({ timeout: 20_000 });
  await expect(page.getByTestId('harmonicity-max')).toContainText('Max');
  await expect(page.getByTestId('harmonicity-max')).toContainText('dB');
  await expect(page.getByTestId('harmonicity-min')).toContainText('Min');
});

test('cpp extrema: min/max CPP over the selection appear when the layer is on', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);
  await page.getByTestId('toggle-cpp').click();

  const extrema = page.getByTestId('cpp-extrema');
  await expect(extrema).toBeVisible({ timeout: 20_000 });
  await expect(page.getByTestId('cpp-max')).toContainText('Max');
  await expect(page.getByTestId('cpp-max')).toContainText('dB');
  await expect(page.getByTestId('cpp-min')).toContainText('Min');
});

test('formant extrema: per-slot frequency range over the selection appears', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  // The formant layer is on by default, so a selection populates the extrema.
  const extrema = page.getByTestId('formant-extrema');
  await expect(extrema).toBeVisible({ timeout: 20_000 });
  const rows = page.getByTestId('formant-extrema-row');
  await expect(rows.first()).toContainText('F1');
  await expect(rows.first()).toContainText('Hz');
  expect(await rows.count()).toBeGreaterThan(0);
});

test('formant bandwidths: the selection-mean table reports Fn and Bn per slot', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);

  // The per-slot mean table needs the tracked (smoothed) formants, which carry
  // the cross-frame identity a mean is taken along. The section is expanded by
  // default, so turn tracking on, then select a span.
  await page.getByTestId('formant-smoothed').check();
  await dragSpectrogramBox(page);

  const table = page.getByTestId('formant-mean-table');
  await expect(table).toBeVisible({ timeout: 20_000 });
  await expect(table).toContainText('over selection');
  await expect(table).toContainText('Bw');

  // The first slot carries a finite mean frequency and a finite mean bandwidth.
  const firstRow = page.getByTestId('formant-mean-row').first();
  await expect(firstRow).toContainText('F1');
  const bandwidth = Number(
    ((await firstRow.locator('td').nth(2).textContent()) ?? '').replace(/[^0-9.]/g, '')
  );
  expect(bandwidth).toBeGreaterThan(0);
});

test('spectrogram settings: changing the window length re-renders the spectrogram', async ({
  page
}) => {
  await openEditorWithFixture(page, vowelFixture);
  const canvas = page.getByTestId('spectrogram-canvas');
  await expect(canvas).toBeVisible();

  // Expand the Spectrogram settings section and read the current render.
  await page.getByTestId('inspector-spectrogram').getByRole('button', { name: 'Spectrogram' }).click();
  const window = page.getByTestId('spectrogram-window');
  await expect(window).toBeVisible();
  const snapshot = () => canvas.evaluate((el: HTMLCanvasElement) => el.toDataURL());
  const before = await snapshot();

  // Switch to a narrowband window; the raster changes.
  await window.fill('0.03');
  await window.blur();
  await expect.poll(async () => (await snapshot()) !== before, { timeout: 20_000 }).toBe(true);
});

test('cpp overlay: the layer toggles on', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  const eye = page.getByTestId('toggle-cpp');
  await expect(eye).toHaveAttribute('aria-pressed', 'false');
  await eye.click();
  await expect(eye).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('inspector-cpp')).not.toHaveClass(/\boff\b/);
});

test('copy CPP contour writes CSV rows to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('cpp');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="copyCppContour"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('editor-toast')).toContainText('CPP copied', { timeout: 20_000 });
  const csv = await page.evaluate(() => navigator.clipboard.readText());
  expect(csv.startsWith('time_s,cpp_db')).toBe(true);
  const lines = csv.trim().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  const [t, cpp] = lines[1].split(',').map(Number);
  expect(Number.isFinite(t)).toBe(true);
  expect(Number.isFinite(cpp)).toBe(true);
});

test('concatenate joins the project recordings into a new one', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  // A second recording, so concatenate has something to join.
  await dragSpectrogramBox(page);
  await page.getByTestId('selection-extract').click();
  await expect(page.getByTestId('recording-switcher-name')).toContainText('[', { timeout: 20_000 });

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('concatenate');
  const cmd = page.locator(
    '[data-testid="command-item"][data-command-id="concatenateRecordings"]'
  );
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('concatenated', {
    timeout: 30_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(3);
});

test('combine to stereo merges the current recording with another', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  // Extract a selection so the project holds a second recording to pair with.
  await dragSpectrogramBox(page);
  await page.getByTestId('selection-extract').click();
  await expect(page.getByTestId('recording-switcher-name')).toContainText('[', { timeout: 20_000 });

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('combine to stereo');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="combineToStereo"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[stereo]', {
    timeout: 30_000
  });
});

test('notch filter stores a band-stopped copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('stop Hann band');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="notchSelection"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[notch', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('pre-emphasis stores a high-passed copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('pre-emphasize selection');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="preemphasisSelection"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[pre-emphasis]', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(2);
});

test('subtract mean stores a DC-removed copy as a new recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('subtract mean');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="subtractMeanSelection"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[DC removed]', {
    timeout: 20_000
  });
});

test('extract channels: the command is hidden for a mono recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('extract each channel');
  await expect(
    page.locator('[data-testid="command-item"][data-command-id="extractChannels"]')
  ).toHaveCount(0);
});

test('extract channels: a stereo take splits into per-channel recordings', async ({ page }) => {
  await openEditorWithFixture(page, stereoFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('extract each channel');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="extractChannels"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[ch', {
    timeout: 20_000
  });
  await page.getByTestId('recording-switcher').click();
  // The stereo original plus one recording per channel.
  await expect(
    page.getByTestId('recording-switcher-popover').getByTestId('switcher-option')
  ).toHaveCount(3);
});

test('convert to mono: a stereo take mixes down to a new recording', async ({ page }) => {
  await openEditorWithFixture(page, stereoFixture);

  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('convert to mono');
  const cmd = page.locator('[data-testid="command-item"][data-command-id="convertToMono"]');
  await expect(cmd).toBeVisible();
  await cmd.hover();
  await page.keyboard.press('Enter');

  await expect(page.getByTestId('recording-switcher-name')).toContainText('[mono]', {
    timeout: 20_000
  });
});

test('convert to mono: the command is hidden for a mono recording', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await page.keyboard.press('Control+k');
  await page.getByTestId('command-palette-input').fill('convert to mono');
  await expect(
    page.locator('[data-testid="command-item"][data-command-id="convertToMono"]')
  ).toHaveCount(0);
});

test('batch equals GUI: readout band energy equals a direct engine query', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  const coords = await dragSpectrogramBox(page);

  const shown = Number(await page.getByTestId('readout-band-energy').getAttribute('data-value'));
  expect(Number.isFinite(shown)).toBe(true);

  // Direct engine query at the identical coordinates, through the live client
  // the app drives — never a frontend recomputation.
  const direct = await page.evaluate(async (box) => {
    const hook = (globalThis as unknown as { __phonia?: { client: any; audioId: bigint | null } })
      .__phonia;
    if (!hook || hook.audioId === null) throw new Error('no client hook');
    return (await hook.client.bandEnergy(hook.audioId, box.t0, box.t1, box.f0, box.f1)) as number;
  }, coords);

  expect(Math.abs(shown - direct)).toBeLessThan(1e-6);
});

test('selection box stays anchored in signal coordinates across zoom', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  const boxEl = spectrogramBox(page);
  const beforeT0 = await boxEl.getAttribute('data-sel-t0');
  const beforeRect = await boxEl.boundingBox();
  if (!beforeRect) throw new Error('selection box not rendered');
  const beforeSpanEnd = await page.getByTestId('editor').getAttribute('data-visible-end');

  // Zoom in ~4x centred on the box, then confirm the box kept its signal-space
  // start while its pixel position re-mapped under the new viewport.
  const canvas = page.getByTestId('spectrogram-canvas');
  const cbox = await canvas.boundingBox();
  if (!cbox) throw new Error('no canvas');
  await page.mouse.move(cbox.x + cbox.width * 0.5, cbox.y + cbox.height * 0.5);
  for (let i = 0; i < 7; i += 1) await page.mouse.wheel(0, -100);

  await expect
    .poll(async () => page.getByTestId('editor').getAttribute('data-visible-end'))
    .not.toBe(beforeSpanEnd);

  await expect(boxEl).toHaveAttribute('data-sel-t0', beforeT0 ?? '');
  const afterRect = await boxEl.boundingBox();
  expect(afterRect).not.toBeNull();
  expect(Math.abs((afterRect?.x ?? 0) - beforeRect.x)).toBeGreaterThan(1);
});

test('voice report on the sustained vowel reports plausible values', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);

  await page.getByTestId('selection-voice-report').click();
  const card = page.getByTestId('voice-report-card');
  await expect(card).toBeVisible();
  const values = page.getByTestId('voice-report-values');
  await expect(values).toBeVisible({ timeout: 30_000 });

  const jitter = Number(await values.getAttribute('data-jitter-local'));
  const shimmer = Number(await values.getAttribute('data-shimmer-local'));
  const hnr = Number(await values.getAttribute('data-hnr'));

  // A clean synthetic vowel: perturbation near its ~0 injection level, HNR high.
  expect(jitter).toBeGreaterThanOrEqual(0);
  expect(jitter).toBeLessThan(0.05);
  expect(shimmer).toBeGreaterThanOrEqual(0);
  expect(shimmer).toBeLessThan(0.2);
  expect(hnr).toBeGreaterThan(10);
});

test('F fits the selection, 0 fits the file', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  const editor = page.getByTestId('editor');

  await dragSpectrogramBox(page);
  const selT0 = Number(await spectrogramBox(page).getAttribute('data-sel-t0'));
  expect(selT0).toBeGreaterThan(0);

  // F frames the selection: the viewport start moves onto the box start. The
  // window keydown handler reaches the editor without a focused control.
  await page.keyboard.press('f');
  await expect
    .poll(() => editor.getAttribute('data-visible-start').then(Number))
    .toBeGreaterThan(0.01);

  // 0 fits the whole file: the viewport start returns to the signal origin.
  await page.keyboard.press('0');
  await expect
    .poll(() => editor.getAttribute('data-visible-start').then(Number))
    .toBeLessThan(0.01);
});

test('selection and voice report: light and dark screenshots', async ({ page }) => {
  await openEditorWithFixture(page, vowelFixture);
  await dragSpectrogramBox(page);
  await page.getByTestId('selection-voice-report').click();
  await expect(page.getByTestId('voice-report-values')).toBeVisible({ timeout: 30_000 });

  // Light.
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(screenshots, 'selection-voice-report-light.png'), fullPage: true });

  // Dark: close the card (Escape keeps the selection), flip to dark, re-open.
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('voice-report-card')).toHaveCount(0);
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect
    .poll(() => page.evaluate(() => document.documentElement.classList.contains('dark')))
    .toBe(true);
  await page.getByTestId('selection-voice-report').click();
  await expect(page.getByTestId('voice-report-values')).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(screenshots, 'selection-voice-report-dark.png'), fullPage: true });
});

import { expect, type Page } from '@playwright/test';

/**
 * Enters the editor for a single WAV fixture through the home screen.
 *
 * The app opens on the project manager; dropping (here, selecting) a WAV creates
 * a project, and clicking its corpus row opens that recording in the editor.
 */
export async function openEditorWithFixture(page: Page, wavPath: string): Promise<void> {
  // Pre-answer the first-run keyboard-mode prompt so it never races the editor
  // controls a test is about to click; specs that exercise the prompt itself
  // seed their own value and do not use this helper.
  await page.addInitScript(() => localStorage.setItem('phonia:key-mode', 'phonia'));
  // `?app=1` is the landing page's own "Open Phonia" link: it skips the
  // first-visit marketing page and lands straight in the app, which is what
  // every e2e test wants regardless of Playwright's fresh per-test storage.
  await page.goto('/?app=1');
  await page.getByTestId('folder-input').setInputFiles([wavPath]);
  await expect(page.getByTestId('corpus')).toBeVisible();
  await page.getByTestId('corpus-row').first().click();
  await expect(page.getByTestId('editor')).toHaveAttribute('data-visible-end', /[1-9]/);
}

/**
 * A newly added tier opens its own name field focused. Specs that go on
 * typing dismiss it with Escape; waiting for the field to be focused first
 * makes the add-then-type sequence deterministic.
 */
export async function dismissTierName(page: Page): Promise<void> {
  const field = page.getByTestId('tier-name-input');
  await expect(field).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(field).toBeHidden();
}

/**
 * Counts painted canvas pixels — those departing from the pane's dominant
 * background colour. The WebGL panes draw hard-edged (no anti-aliasing), so
 * unique-colour counting can't distinguish real content from a flat fill;
 * coverage does, and it is independent of amplitude, palette, or cursor state.
 */
export async function canvasForegroundCoverage(page: Page, testId: string) {
  return page.getByTestId(testId).evaluate(async (canvas: HTMLCanvasElement) => {
    const bitmap = await createImageBitmap(canvas);
    const w = bitmap.width;
    const h = bitmap.height;
    const off = new OffscreenCanvas(w, h);
    const ctx = off.getContext('2d');
    if (!ctx) return 0;
    ctx.drawImage(bitmap, 0, 0);
    const pixels = ctx.getImageData(0, 0, w, h).data;
    const tally = new Map<number, number>();
    const key = (i: number) =>
      (pixels[i] << 24) | (pixels[i + 1] << 16) | (pixels[i + 2] << 8) | pixels[i + 3];
    for (let i = 0; i < pixels.length; i += 4) {
      const k = key(i);
      tally.set(k, (tally.get(k) ?? 0) + 1);
    }
    let backgroundCount = 0;
    for (const count of tally.values()) if (count > backgroundCount) backgroundCount = count;
    const total = pixels.length / 4;
    return total - backgroundCount;
  });
}

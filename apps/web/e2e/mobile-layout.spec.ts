import { expect, test, type Page } from '@playwright/test';

test.use({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  hasTouch: true,
  isMobile: true
});

async function expectNoHorizontalOverflow(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(() => ({
        viewport: document.documentElement.clientWidth,
        content: document.documentElement.scrollWidth
      }))
    )
    .toEqual({ viewport: 390, content: 390 });
}

test('smartphone layout keeps navigation and analysis controls usable', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('phonia:key-mode', 'phonia');
    localStorage.setItem('phonia:pwa-install-dismissed', '1');
  });
  await page.goto('/?app=1');

  const modes = page.getByRole('navigation', { name: 'Modes' });
  await expect(modes).toBeVisible();
  const modeBox = await modes.boundingBox();
  expect(modeBox?.width).toBe(390);
  expect(modeBox?.y).toBeGreaterThanOrEqual(779);

  const create = page.getByTestId('new-project');
  const createBox = await create.boundingBox();
  expect(createBox?.height).toBeGreaterThanOrEqual(44);
  await expectNoHorizontalOverflow(page);

  await page.getByTestId('open-sample').click();
  await expect(page.getByTestId('corpus-row')).toHaveCount(3, { timeout: 30_000 });
  await expect(page.getByTestId('corpus-search')).toBeVisible();
  await expectNoHorizontalOverflow(page);

  await page.getByTestId('corpus-row').first().click();
  await expect(page.getByTestId('editor')).toHaveAttribute('data-visible-end', /[1-9]/);
  await expect(page.getByTestId('recordings-rail')).toBeHidden();
  await expect(page.getByTestId('level-meter')).toBeHidden();

  const timeline = page.getByTestId('timeline');
  const timelineBox = await timeline.boundingBox();
  expect(timelineBox?.width).toBeGreaterThanOrEqual(380);
  expect(timelineBox?.height).toBeGreaterThan(300);
  await expectNoHorizontalOverflow(page);

  const beforeSpan = await page.getByTestId('editor').evaluate((node) =>
    Number(node.getAttribute('data-visible-end')) - Number(node.getAttribute('data-visible-start'))
  );
  await page.getByTestId('selection-layer-time').evaluate((node) => {
    const rect = node.getBoundingClientRect();
    const send = (type: string, pointerId: number, clientX: number) =>
      node.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId,
          pointerType: 'touch',
          button: 0,
          buttons: type === 'pointerup' ? 0 : 1,
          clientX,
          clientY: rect.top + rect.height / 2
        })
      );
    const center = rect.left + rect.width / 2;
    send('pointerdown', 11, center - 40);
    send('pointerdown', 12, center + 40);
    send('pointermove', 12, center + 100);
    send('pointerup', 12, center + 100);
    send('pointerup', 11, center - 40);
  });
  await expect
    .poll(() =>
      page.getByTestId('editor').evaluate((node) =>
        Number(node.getAttribute('data-visible-end')) - Number(node.getAttribute('data-visible-start'))
      )
    )
    .toBeLessThan(beforeSpan);

  await page.getByRole('button', { name: 'Plots' }).click();
  await expect(page.getByTestId('plots-view')).toBeVisible();
  const plotsCanvasBox = await page.getByTestId('plots-canvas').boundingBox();
  expect(plotsCanvasBox?.width).toBe(390);
  expect(plotsCanvasBox?.height).toBeGreaterThan(300);
  await expectNoHorizontalOverflow(page);
});

test('touch input supports tier creation, labeling, and boundary dragging', async ({
  page,
  context
}) => {
  await page.addInitScript(() => {
    localStorage.setItem('phonia:key-mode', 'phonia');
    localStorage.setItem('phonia:pwa-install-dismissed', '1');
  });
  await page.goto('/?app=1');
  await page.getByTestId('open-sample').tap();
  await expect(page.getByTestId('corpus-row')).toHaveCount(3, { timeout: 30_000 });
  await page.getByTestId('corpus-row').first().tap();
  await expect(page.getByTestId('tier-pane')).toBeVisible();

  await page.getByTestId('add-interval-tier').tap();
  await page.getByTestId('tier-name-input').fill('mobile words');
  await page.getByTestId('tier-name-input').press('Enter');
  const wordsLane = page.locator('[data-testid="tier-lane"][data-tier-name="mobile words"]');
  await expect(wordsLane).toBeVisible();

  const wave = await page.getByTestId('waveform-canvas').boundingBox();
  if (!wave) throw new Error('waveform has no box');
  await page.touchscreen.tap(wave.x + wave.width * 0.6, wave.y + wave.height / 2);
  await expect(page.getByTestId('editor')).not.toHaveAttribute('data-cursor-time', '0.000000');
  await page.getByTestId('split-at-cursor').tap();
  await expect(wordsLane.getByTestId('interval')).toHaveCount(2);

  await page.getByTestId('edit-label').tap();
  await page.getByTestId('label-editor').fill('ta');
  await page.getByTestId('label-editor').press('Enter');
  await expect(wordsLane.getByTestId('interval').filter({ hasText: 'ta' })).toHaveCount(1);

  const handle = wordsLane.getByTestId('boundary-handle');
  await handle.scrollIntoViewIfNeeded();
  const handleBox = await handle.boundingBox();
  if (!handleBox) throw new Error('boundary handle has no box');
  const before = Number(await wordsLane.getByTestId('interval').first().getAttribute('data-xmax'));
  const x = handleBox.x + handleBox.width / 2;
  const y = handleBox.y + handleBox.height * 0.8;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x, y, id: 1 }]
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ x: x + 36, y, id: 1 }]
  });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });

  await expect
    .poll(async () => Number(await wordsLane.getByTestId('interval').first().getAttribute('data-xmax')))
    .toBeGreaterThan(before);
  expect((await page.getByTestId('tier-status').allTextContents()).join(' ')).not.toContain(
    'annotation mutation failed'
  );
  await expectNoHorizontalOverflow(page);
});

import { expect, test } from '@playwright/test';

test.use({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  hasTouch: true,
  isMobile: true
});

test('browser install prompt is available from the app', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('phonia:key-mode', 'phonia'));
  await page.goto('/?app=1');
  await page.getByTestId('pwa-status-ready').waitFor({ state: 'attached' });
  await page.evaluate(() => {
    const event = new Event('beforeinstallprompt', { cancelable: true });
    Object.assign(event, {
      prompt: async () => {
        (window as typeof window & { __installPrompted?: boolean }).__installPrompted = true;
      },
      userChoice: Promise.resolve({ outcome: 'accepted', platform: 'web' })
    });
    window.dispatchEvent(event);
  });
  await expect(page.getByTestId('pwa-install-prompt')).toBeVisible();
  await page
    .getByTestId('pwa-install-prompt')
    .getByRole('button', { name: 'Install', exact: true })
    .click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        Boolean((window as typeof window & { __installPrompted?: boolean }).__installPrompted)
      )
    )
    .toBe(true);
  await expect(page.getByTestId('pwa-install-prompt')).toBeHidden();
});

test('declined browser install stays dismissed', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('phonia:key-mode', 'phonia'));
  await page.goto('/?app=1');
  await page.getByTestId('pwa-status-ready').waitFor({ state: 'attached' });

  const dispatchPrompt = () =>
    page.evaluate(() => {
      const event = new Event('beforeinstallprompt', { cancelable: true });
      Object.assign(event, {
        prompt: async () => {},
        userChoice: Promise.resolve({ outcome: 'dismissed', platform: 'web' })
      });
      window.dispatchEvent(event);
    });

  await dispatchPrompt();
  const prompt = page.getByTestId('pwa-install-prompt');
  await expect(prompt).toBeVisible();
  await prompt.getByRole('button', { name: 'Install', exact: true }).click();
  await expect(prompt).toBeHidden();
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem('phonia:pwa-install-dismissed')))
    .toBe('1');

  await dispatchPrompt();
  await expect(prompt).toBeHidden();
});

test('iPad offers Home Screen instructions', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('phonia:key-mode', 'phonia');
    Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel' });
    Object.defineProperty(navigator, 'maxTouchPoints', { get: () => 5 });
  });
  await page.goto('/?app=1');

  const prompt = page.getByTestId('pwa-install-prompt');
  await expect(prompt).toBeVisible();
  await prompt.getByRole('button', { name: 'Install', exact: true }).click();
  const instructions = page.getByRole('dialog', { name: 'Add Phonia to your Home Screen' });
  await expect(instructions).toBeVisible();
  await expect(instructions).toContainText('Add to Home Screen');
  await instructions.getByRole('button', { name: 'Close' }).click();
  await expect(instructions).toBeHidden();
});

test('cached app shell opens while offline', async ({ page, context }) => {
  await page.addInitScript(() => {
    localStorage.setItem('phonia:key-mode', 'phonia');
    localStorage.setItem('phonia:pwa-install-dismissed', '1');
  });
  await page.goto('/?app=1');
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
  });
  await context.setOffline(true);
  try {
    await page.reload({ waitUntil: 'domcontentloaded' });
    await expect(page.getByTestId('home')).toBeVisible();
    await expect(page.getByTestId('offline-status')).toBeVisible();
  } finally {
    await context.setOffline(false);
  }
});

test('manifest exposes install metadata and app icons', async ({ request }) => {
  const response = await request.get('/manifest.webmanifest');
  expect(response.ok()).toBeTruthy();
  const manifest = await response.json();
  expect(manifest).toMatchObject({
    id: '/?app=1',
    start_url: '/?app=1',
    scope: '/',
    display: 'standalone',
    theme_color: '#0f766e'
  });
  expect(manifest.icons).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ sizes: '192x192', purpose: 'any' }),
      expect.objectContaining({ sizes: '512x512', purpose: 'any' }),
      expect.objectContaining({ sizes: '512x512', purpose: 'maskable' })
    ])
  );
});

import { test, expect } from 'playwright/test';

test.describe('Smoke tests', () => {
  test('landing page loads without runtime errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto('/');
    await page.waitForLoadState('networkidle');
    // Allow redirects to settle
    await page.waitForTimeout(2000);

    expect(errors).toEqual([]);
  });

  test('landing page redirects unauthenticated users to /home', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);

    expect(page.url()).toContain('/home');
  });

  test('marketing page renders key content', async ({ page }) => {
    await page.goto('/home');
    await page.waitForLoadState('networkidle');

    await expect(page.locator('body')).toBeVisible();
    // Page should have some meaningful text content
    const bodyText = await page.textContent('body');
    expect(bodyText?.length).toBeGreaterThan(100);
  });

  test('login page is accessible', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto('/login');
    await page.waitForLoadState('networkidle');

    expect(errors).toEqual([]);
  });

  test('no console errors on key pages', async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error' && !msg.text().includes('Warning:')) {
        consoleErrors.push(msg.text());
      }
    });

    const pages = ['/', '/home', '/login'];
    for (const path of pages) {
      await page.goto(path);
      await page.waitForLoadState('networkidle');
      await page.waitForTimeout(1000);
    }

    expect(consoleErrors).toEqual([]);
  });
});

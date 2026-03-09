// playwright example
import { test, expect } from '@playwright/test';

test('user registers, uploads vendor data, and matches vendors', async ({ page }) => {
    // Register new user
    await page.goto('/register');
    await page.fill('#username', 'testuser');
    await page.fill('#email', 'test@example.com');
    await page.fill('#password', 'password123');
    await page.click('#register-button');

    // Upload vendor data
    await page.goto('/upload');
    await page.setInputFiles('#vendor-data-file', 'path/to/vendor-data.json');
    await page.click('#submit-button');

    // Initiate matching process
    await page.goto('/match');
    await page.click('#start-matching-button');

    // Verify matched vendors displayed
    const matchedVendors = await page.locator('.matched-vendor').allTextContents();
    expect(matchedVendors).toContain('Example Vendor');
});
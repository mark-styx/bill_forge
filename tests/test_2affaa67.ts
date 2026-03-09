// e2e/tests/invoice.test.ts
import { test, expect } from '@playwright/test';

test('should create and retrieve an invoice', async ({ page }) => {
    await page.goto('/api/invoices');

    // Create an invoice
    const createButton = await page.locator('#create-invoice');
    await createButton.click();
    await page.fill('#amount', '250.00');
    await page.fill('#description', 'Sample Invoice');
    await page.click('#submit');

    // Verify invoice creation
    await expect(page.locator('#invoice-list').innerText()).toContain('Sample Invoice');

    // Retrieve an invoice
    const retrieveButton = await page.locator('#retrieve-invoice');
    await retrieveButton.click();
    await page.fill('#invoice-id', '1');
    await page.click('#submit');

    // Verify invoice retrieval
    await expect(page.locator('#invoice-details').innerText()).toContain('Sample Invoice');
});
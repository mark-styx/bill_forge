import { test, expect } from '@playwright/test';

test('should upload a document, process it, and download', async ({ page }) => {
    // Arrange
    await page.goto('http://localhost:3000');

    // Act
    await page.locator('#upload-button').click();
    await page.setInputFiles('#document-input', 'path/to/valid/document.pdf');
    await page.click('#process-button');

    await expect(page.locator('#confirmation-message')).toContainText('Processing complete');
    await page.click('#download-button');

    // Assert
    const [response] = await Promise.all([
        page.waitForEvent('requestfinished'),
        page.click('#download-link')
    ]);

    const blob = await response.blob();
    expect(blob.type).toBe('application/pdf');
});
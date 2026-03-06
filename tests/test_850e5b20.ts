import { test, expect } from '@playwright/test';
import { login, navigateToInvoicePage, uploadInvalidFile, checkProcessingStatus } from '../utils';

test('failure scenario for uploading an invalid file format', async ({ page }) => {
    await login(page);
    await navigateToInvoicePage(page);
    
    await uploadInvalidFile(page, 'path/to/invalid_file.txt');
    await checkProcessingStatus(page);
    const error = await page.textContent('#error-message');
    expect(error).toContain('Invalid file format');
});
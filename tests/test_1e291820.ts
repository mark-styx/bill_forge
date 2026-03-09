import { test, expect } from '@playwright/test';

test('should upload an invoice and display line items', async ({ page }) => {
  // Navigate to the frontend page
  await page.goto('http://localhost:3000');

  // Upload an invoice image
  await page.fill('#file-upload', 'path/to/sample/image.jpg');
  await page.click('#upload-button');

  // Assert that line items are displayed correctly
  const lineItems = await page.textContents('#line-items li');
  expect(lineItems).toContain('Item 1');
  expect(lineItems).toContain('Item 2');
});

test('should display error message for invalid file', async ({ page }) => {
  // Navigate to the frontend page
  await page.goto('http://localhost:3000');

  // Upload an invalid invoice image
  await page.fill('#file-upload', 'path/to/invalid/image.jpg');
  await page.click('#upload-button');

  // Assert that error message is displayed
  const errorMessage = await page.textContent('#error-message');
  expect(errorMessage).toContain('Failed to upload file');
});
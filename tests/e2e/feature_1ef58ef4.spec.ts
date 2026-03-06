import { test, expect } from '@playwright/test';

test('user selects Tesseract Local provider', async ({ page }) => {
  await page.goto('http://localhost:3000/invoice-processing');
  
  // Select Tesseract Local provider
  await page.click('#ocr-provider-select');
  await page.selectOption('#ocr-provider-select', 'tesseract_local');

  // Upload a sample invoice
  await page.setInputFiles('#invoice-file-input', 'path/to/sample/invoice.pdf');

  // Submit the form
  await page.click('#submit-invoice-button');

  // Verify OCR results are displayed
  const ocrResults = await page.textContent('#ocr-results');
  expect(ocrResults).toContain('Sample OCR results from Tesseract Local');
});

test('user selects AWS Textract provider', async ({ page }) => {
  await page.goto('http://localhost:3000/invoice-processing');

  // Select AWS Textract provider
  await page.click('#ocr-provider-select');
  await page.selectOption('#ocr-provider-select', 'aws_textract');

  // Upload a sample invoice
  await page.setInputFiles('#invoice-file-input', 'path/to/sample/invoice.pdf');

  // Submit the form
  await page.click('#submit-invoice-button');

  // Verify OCR results are displayed
  const ocrResults = await page.textContent('#ocr-results');
  expect(ocrResults).toContain('Sample OCR results from AWS Textract');
});

test('user selects Google Vision provider', async ({ page }) => {
  await page.goto('http://localhost:3000/invoice-processing');

  // Select Google Vision provider
  await page.click('#ocr-provider-select');
  await page.selectOption('#ocr-provider-select', 'google_vision');

  // Upload a sample invoice
  await page.setInputFiles('#invoice-file-input', 'path/to/sample/invoice.pdf');

  // Submit the form
  await page.click('#submit-invoice-button');

  // Verify OCR results are displayed
  const ocrResults = await page.textContent('#ocr-results');
  expect(ocrResults).toContain('Sample OCR results from Google Vision');
});

test('error handling when provider fails', async ({ page }) => {
  await page.goto('http://localhost:3000/invoice-processing');

  // Select a failing provider (e.g., 'non_existent_provider')
  await page.click('#ocr-provider-select');
  await page.selectOption('#ocr-provider-select', 'non_existent_provider');

  // Upload a sample invoice
  await page.setInputFiles('#invoice-file-input', 'path/to/sample/invoice.pdf');

  // Submit the form
  await page.click('#submit-invoice-button');

  // Verify error message is displayed
  const errorMessage = await page.textContent('#error-message');
  expect(errorMessage).toContain('Error processing OCR: Provider not found or failed');
});
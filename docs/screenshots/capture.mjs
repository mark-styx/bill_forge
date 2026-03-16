/**
 * Screenshot capture script for BillForge README
 * Usage: npx playwright test docs/screenshots/capture.mjs (or just node)
 */

import { chromium } from 'playwright';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT = __dirname;
const BASE = 'http://127.0.0.1:8002';

const VIEWPORT = { width: 1440, height: 900 };

async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: 2,  // retina-quality
    colorScheme: 'light',
  });
  const page = await context.newPage();

  // ---- Login ----
  console.log('Logging in...');
  await page.goto(`${BASE}/login`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1000);
  await page.screenshot({ path: join(OUT, 'login.png') });
  console.log('  -> login.png');

  // Submit login (form is pre-filled with sandbox creds)
  await page.click('button[type="submit"]');
  await page.waitForURL('**/dashboard', { timeout: 15000 });
  await page.waitForTimeout(2000);

  // Override sandbox org name in sidebar for cleaner screenshots
  await page.evaluate(() => {
    const spans = document.querySelectorAll('span');
    spans.forEach(el => {
      if (el.textContent.trim() === 'Farts') el.textContent = 'Acme Corp';
    });
    // Also fix the welcome message if it contains the org name
    const headings = document.querySelectorAll('h1, h2, h3, p');
    headings.forEach(el => {
      if (el.textContent.includes('Farts')) {
        el.textContent = el.textContent.replace('Farts', 'Acme Corp');
      }
    });
  });

  // Helper to re-apply org name override after navigation
  async function cleanOrgName() {
    await page.evaluate(() => {
      document.querySelectorAll('span').forEach(el => {
        if (el.textContent.trim() === 'Farts') el.textContent = 'Acme Corp';
      });
      document.querySelectorAll('h1, h2, h3, p').forEach(el => {
        if (el.textContent.includes('Farts')) {
          el.textContent = el.textContent.replace(/Farts/g, 'Acme Corp');
        }
      });
    });
    await page.waitForTimeout(100);
  }

  // ---- Dashboard ----
  console.log('Capturing dashboard...');
  await cleanOrgName();
  // Dismiss any toast notifications
  await page.evaluate(() => {
    document.querySelectorAll('[data-sonner-toast], [data-dismiss]').forEach(el => el.remove());
    const toastContainer = document.querySelector('[data-sonner-toaster]');
    if (toastContainer) toastContainer.remove();
  });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(OUT, 'dashboard.png') });
  console.log('  -> dashboard.png');

  // ---- Invoices ----
  console.log('Capturing invoices...');
  await page.goto(`${BASE}/invoices`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'invoices.png') });
  console.log('  -> invoices.png');

  // ---- Vendors ----
  console.log('Capturing vendors...');
  await page.goto(`${BASE}/vendors`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'vendors.png') });
  console.log('  -> vendors.png');

  // ---- Processing (queues overview) ----
  console.log('Capturing processing...');
  await page.goto(`${BASE}/processing`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'processing.png') });
  console.log('  -> processing.png');

  // ---- Workflows ----
  console.log('Capturing workflows...');
  await page.goto(`${BASE}/processing/workflows`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'workflows.png') });
  console.log('  -> workflows.png');

  // ---- Reports ----
  console.log('Capturing reports...');
  await page.goto(`${BASE}/reports`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'reports.png') });
  console.log('  -> reports.png');

  // ---- Settings ----
  console.log('Capturing settings...');
  await page.goto(`${BASE}/settings`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  await cleanOrgName();
  await page.screenshot({ path: join(OUT, 'settings.png') });
  console.log('  -> settings.png');

  await browser.close();
  console.log('\nDone! Screenshots saved to docs/screenshots/');
}

main().catch((err) => {
  console.error('Screenshot capture failed:', err);
  process.exit(1);
});

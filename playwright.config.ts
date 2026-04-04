import { defineConfig } from 'playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:8002',
    headless: true,
  },
  projects: [
    {
      name: 'chromium',
      use: { browserName: 'chromium' },
    },
    {
      name: 'critical-path',
      testDir: './tests/e2e',
      grep: /critical path/i,
      timeout: 120000,
      retries: 1,
      use: { browserName: 'chromium' },
    },
  ],
});

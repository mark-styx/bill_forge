import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

// We read the files directly to assert structural properties without
// importing the generated module (which would couple the test to TS resolve).

const pkgRoot = path.resolve(__dirname, '..');

describe('openapi.json', () => {
  it('parses and has a non-empty paths object', () => {
    const raw = fs.readFileSync(path.join(pkgRoot, 'openapi.json'), 'utf-8');
    const doc = JSON.parse(raw);
    expect(doc.openapi).toBe('3.1.0');
    expect(doc.paths).toBeDefined();
    expect(Object.keys(doc.paths).length).toBeGreaterThan(0);
  });
});

describe('generated.ts schemas', () => {
  const requiredSchemas = [
    'Invoice',
    'InvoiceLineItemInfo',
    'MoneyInfo',
    'Vendor',
    'ErrorResponse',
    'PaginationInfo',
    'DashboardMetrics',
    'WorkloadResponse',
    'ApproverWorkloadSummary',
  ] as const;

  it('contains all required schema names in components.schemas', () => {
    const raw = fs.readFileSync(path.join(pkgRoot, 'openapi.json'), 'utf-8');
    const doc = JSON.parse(raw);
    const schemas = Object.keys(doc.components.schemas);

    for (const name of requiredSchemas) {
      expect(schemas, `missing schema: ${name}`).toContain(name);
    }
  });

  it('includes external API paths for invoices and webhook-subscriptions', () => {
    const raw = fs.readFileSync(path.join(pkgRoot, 'openapi.json'), 'utf-8');
    const doc = JSON.parse(raw);

    const externalPaths = [
      '/api/external/v1/invoices',
      '/api/external/v1/webhook-subscriptions',
    ] as const;

    for (const p of externalPaths) {
      expect(doc.paths[p], `missing external path: ${p}`).toBeDefined();
    }

    // Verify expected operations exist
    expect(doc.paths['/api/external/v1/invoices'].get).toBeDefined();
    expect(doc.paths['/api/external/v1/webhook-subscriptions'].post).toBeDefined();
    expect(doc.paths['/api/external/v1/webhook-subscriptions'].get).toBeDefined();
  });

  it('generated.ts file exists and references all required schemas', () => {
    const genPath = path.join(pkgRoot, 'src', 'generated.ts');
    expect(fs.existsSync(genPath)).toBe(true);

    const content = fs.readFileSync(genPath, 'utf-8');
    for (const name of requiredSchemas) {
      // openapi-typescript emits schema names as `SchemaName: {` inside `schemas: {`
      expect(content, `generated.ts missing schema: ${name}`).toMatch(
        new RegExp(`\\b${name}\\s*:\\s*\\{`)
      );
    }
  });
});

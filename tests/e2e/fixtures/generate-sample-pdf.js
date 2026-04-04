#!/usr/bin/env node
// Generates a minimal sample invoice PDF for E2E testing.
// Run: node tests/e2e/fixtures/generate-sample-pdf.js
// Output: tests/e2e/fixtures/sample-invoice.pdf

const fs = require('fs');
const path = require('path');

function buildPdf() {
  // Content stream with extractable text
  const contentLines = [
    'BT',
    '/F1 18 Tf',
    '100 750 Td',
    '(INVOICE) Tj',
    '/F1 12 Tf',
    '0 -30 Td',
    '(Vendor: Acme Test Corporation) Tj',
    '0 -20 Td',
    '(Invoice Number: INV-E2E-001) Tj',
    '0 -20 Td',
    '(Invoice Date: 2025-03-15) Tj',
    '0 -20 Td',
    '(Due Date: 2025-04-15) Tj',
    '0 -40 Td',
    '(Item: Widget A - Quantity: 10 - Unit Price: $25.00) Tj',
    '0 -20 Td',
    '(Item: Widget B - Quantity: 5 - Unit Price: $50.00) Tj',
    '0 -30 Td',
    '/F1 14 Tf',
    '(Subtotal: $500.00) Tj',
    '0 -20 Td',
    '(Tax: $40.00) Tj',
    '0 -20 Td',
    '(Total Amount: $540.00) Tj',
    'ET',
  ];
  const streamContent = contentLines.join('\n');

  // Build raw PDF bytes with correct cross-reference offsets
  const header = '%PDF-1.4\n';

  const objects = [
    {
      num: 1,
      content: '<< /Type /Catalog /Pages 2 0 R >>',
    },
    {
      num: 2,
      content: '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    },
    {
      num: 3,
      content:
        '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]\n   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    },
    {
      num: 4,
      content: `<< /Length ${Buffer.byteLength(streamContent, 'latin1')} >>\nstream\n${streamContent}\nendstream`,
    },
    {
      num: 5,
      content: '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    },
  ];

  // Assemble body, tracking byte offsets
  let body = '';
  const offsets = {};
  for (const obj of objects) {
    offsets[obj.num] = Buffer.byteLength(header + body, 'latin1');
    body += `${obj.num} 0 obj\n${obj.content}\nendobj\n`;
  }

  const xrefOffset = Buffer.byteLength(header + body, 'latin1');
  const maxObj = Math.max(...objects.map((o) => o.num));

  let xref = `xref\n0 ${maxObj + 1}\n`;
  xref += '0000000000 65535 f \n';
  for (let i = 1; i <= maxObj; i++) {
    xref += String(offsets[i] || 0).padStart(10, '0') + ' 00000 n \n';
  }

  xref += `trailer\n<< /Size ${maxObj + 1} /Root 1 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;

  return Buffer.from(header + body + xref, 'latin1');
}

const outDir = path.join(__dirname);
const outFile = path.join(outDir, 'sample-invoice.pdf');

fs.writeFileSync(outFile, buildPdf());
console.log(`Generated: ${outFile} (${fs.statSync(outFile).size} bytes)`);

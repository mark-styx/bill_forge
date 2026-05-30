// Post-processes generated.ts to add @ts-nocheck (duplicate operationIds across
// merged feature-gated utoipa docs produce TS2300 errors; the fix belongs in
// the backend, not in generated output).
const fs = require('fs');
const path = require('path');
const outPath = path.resolve(__dirname, '..', 'src', 'generated.ts');
let content = fs.readFileSync(outPath, 'utf-8');
if (!content.startsWith('// @ts-nocheck')) {
  content = '// @ts-nocheck\n' + content;
  fs.writeFileSync(outPath, content);
}

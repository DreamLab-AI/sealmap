// Validate every ```mermaid block in the given Markdown files with Pele, the
// estate's diagram renderer (VisionFlow ADR-2020): sha256-pinned in
// docs/diagrams/tools/vendor/pele, in-process, no browser and no npm install.
// Usage: sealmap generate . -o .sealmap && node tools/validate-mermaid.mjs $(find .sealmap -name '*.md')
// Env: MODE=render for a full render (layout faults surface only there); default parse.
import fs from 'fs';
import { createRequire } from 'module';
const peleStatic = createRequire(import.meta.url)('../docs/diagrams/tools/lib/pele-static.cjs');
const pele = await peleStatic.loadPele();
const MODE = process.env.MODE || 'parse';
const blocks = [];
for (const f of process.argv.slice(2)) {
  const t = fs.readFileSync(f, 'utf8');
  const re = /```mermaid\n([\s\S]*?)```/g; let m;
  while ((m = re.exec(t))) blocks.push({ f, code: m[1] });
}
let bad = 0;
for (const b of blocks) {
  try {
    if (MODE === 'render') peleStatic.renderStatic(pele, b.code);
    else pele.parse(b.code);
  } catch (e) {
    bad++;
    if (bad <= 15) console.log(`FAIL ${b.f}\n${peleStatic.describeError(e)}\n---\n${b.code.split('\n').slice(0, 40).join('\n')}\n`);
  }
}
console.log(`${blocks.length} diagrams, ${bad} failed`);
process.exit(bad ? 1 : 0);

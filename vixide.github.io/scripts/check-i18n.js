// Fails if a home-page locale catalog (src/lib/i18n/locales/*.json) has a key
// that en.json lacks, a {placeholder} or <k>/<c>/<a> markup tag that the
// English string does not have, or a missing/unexpected locale file.
// Languages may omit keys (the page falls back to English per string); the
// report counts them.
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const dir = join(dirname(fileURLToPath(import.meta.url)), '../src/lib/i18n/locales');
const expected = ['ar', 'bn', 'cy', 'de', 'en', 'es', 'fr', 'ga', 'gd', 'hi', 'ja', 'pl', 'pt', 'ru', 'zh'];
const load = (code) => JSON.parse(readFileSync(join(dir, `${code}.json`), 'utf8'));
const tokens = (s) => ({
  placeholders: [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort(),
  tags: [...s.matchAll(/<(\/?)([kca])>/g)].map((m) => m[1] + m[2]).sort()
});

const files = readdirSync(dir)
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.slice(0, -5))
  .sort();
const errors = [];
for (const code of expected) if (!files.includes(code)) errors.push(`missing locale file: ${code}.json`);
for (const code of files) if (!expected.includes(code)) errors.push(`unexpected locale file: ${code}.json`);

const en = load('en');
for (const code of files) {
  const cat = load(code);
  const missing = Object.keys(en).filter((key) => !(key in cat)).length;
  for (const [key, value] of Object.entries(cat)) {
    if (!(key in en)) {
      errors.push(`${code}: key "${key}" is not in en.json`);
      continue;
    }
    if (typeof value !== 'string' || value.trim() === '') {
      errors.push(`${code}: "${key}" is empty`);
      continue;
    }
    const want = tokens(en[key]);
    const got = tokens(value);
    for (const p of got.placeholders)
      if (!want.placeholders.includes(p)) errors.push(`${code}: "${key}" has unknown placeholder {${p}}`);
    if (want.placeholders.join() !== got.placeholders.join())
      errors.push(`${code}: "${key}" placeholders differ from en`);
    if (want.tags.join() !== got.tags.join()) errors.push(`${code}: "${key}" markup tags differ from en`);
  }
  if (code !== 'en') {
    const note = missing ? ` (${missing} fall back to English)` : '';
    console.log(`${code}: ${Object.keys(cat).length}/${Object.keys(en).length} strings${note}`);
  }
}
if (errors.length) {
  console.error(errors.join('\n'));
  process.exit(1);
}
console.log(`i18n catalogs OK (${files.length} locales, ${Object.keys(en).length} keys)`);

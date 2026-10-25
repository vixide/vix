// Copy the Lily reference theme stylesheets from the
// `@lilydesignsystem/themes` package into `static/assets/themes/`, where
// ThemePicker's managed <link> loads them by URL (`themesUrl` in
// +layout.svelte). The copies are build output, not source: the directory
// is gitignored and recreated on every `dev`/`build`, so the site always
// ships exactly the theme files of the installed package version — never
// hand-edit one, upgrade the package instead. See index.md "Themes".

import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = dirname(fileURLToPath(import.meta.resolve('@lilydesignsystem/themes/light.css')));
const target = fileURLToPath(new URL('../static/assets/themes/', import.meta.url));

const files = readdirSync(source).filter((name) => name.endsWith('.css'));
if (files.length === 0) {
  throw new Error(`no theme stylesheets found in ${source}`);
}

if (existsSync(target)) rmSync(target, { recursive: true });
mkdirSync(target, { recursive: true });
for (const name of files) cpSync(join(source, name), join(target, name));

console.log(`copied ${files.length} Lily themes into static/assets/themes/`);

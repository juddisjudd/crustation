// Writes app/translations/ui/_template.json: every English key, flattened.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const { en } = await import(pathToFileURL(resolve(here, '../src/lib/i18n/messages/index.ts')).href);

function flatten(source, prefix = '', target = {}) {
	for (const [key, value] of Object.entries(source)) {
		const path = prefix ? `${prefix}.${key}` : key;
		if (typeof value === 'string') target[path] = value;
		else if (value && typeof value === 'object') flatten(value, path, target);
	}
	return target;
}

const out = resolve(here, '../../../translations/ui/_template.json');
mkdirSync(dirname(out), { recursive: true });
const flat = flatten(en);
writeFileSync(out, JSON.stringify(flat, null, '\t') + '\n');
console.log(`${Object.keys(flat).length} keys → ${out}`);

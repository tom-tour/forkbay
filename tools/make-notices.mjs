// Generates assets/THIRD-PARTY.txt: license notices of every library compiled into the program.
// Run from the repository root: node tools/make-notices.mjs
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const meta = JSON.parse(execSync('cargo metadata --format-version 1 --locked', { maxBuffer: 1 << 28 }));
const used = new Set();
// Only what the two published programs really contain.
for (const target of ['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu']) {
  const tree = execSync(`cargo tree -e normal --prefix none --target ${target} --locked`, { maxBuffer: 1 << 28 }).toString();
  tree.split('\n').map(l => l.split(' ').slice(0, 2).join(' ').trim()).filter(Boolean).forEach(x => used.add(x));
}
const crates = meta.packages.filter(p => p.name !== 'forkbay' && used.has(`${p.name} v${p.version}`)).sort((a, b) => a.name.localeCompare(b.name));
const home = /^\s*$/;

const groups = new Map();
const missing = [];
for (const c of crates) {
  const dir = path.dirname(c.manifest_path);
  const files = fs.readdirSync(dir).filter(f => /^(licen[cs]e|copying|notice|unlicense)/i.test(f)).sort();
  let text = files.map(f => fs.readFileSync(path.join(dir, f), 'utf8').trim()).filter(t => !home.test(t)).join('\n\n---\n\n');
  // Some crates publish their license only in their repository: see assets/notices.
  const extra = `assets/notices/${c.name}.txt`;
  if (fs.existsSync(extra)) text = [text, fs.readFileSync(extra, 'utf8').trim()].filter(Boolean).join('\n\n---\n\n');
  if (!text) { missing.push(c.name); text = `License: ${c.license}. See ${c.repository ?? 'the crate page on crates.io'}.`; }
  if (!groups.has(text)) groups.set(text, []);
  groups.get(text).push(`${c.name} ${c.version} (${c.license})`);
}

let out = 'Forkbay includes the following open source libraries.\n';
out += 'Each one keeps its own license, reproduced below.\n';
for (const [text, names] of groups) {
  out += '\n' + '='.repeat(72) + '\n' + names.join('\n') + '\n' + '-'.repeat(72) + '\n' + text + '\n';
}
fs.writeFileSync('assets/THIRD-PARTY.txt', out.replace(/\r\n/g, '\n'));
console.log(`${crates.length} libraries, ${groups.size} distinct texts, ${out.length} characters. Without a license file: ${missing.join(', ') || 'none'}.`);
if (/[A-Za-z]:\\Users|\/home\//.test(out)) { console.log('WARNING: a local path is in the notices'); process.exit(1); }

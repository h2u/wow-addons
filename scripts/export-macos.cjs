const fs = require('node:fs'),
  path = require('node:path'),
  crypto = require('node:crypto');
if (process.platform !== 'darwin') throw new Error('Export macOS assets on macOS.');
const root = path.resolve(__dirname, '..'),
  version = require('../package.json').version;
const source = path.join(root, 'target/universal-apple-darwin/release/bundle/dmg');
const files = fs.readdirSync(source).filter((f) => f.endsWith('.dmg'));
if (files.length !== 1) throw new Error('Expected one universal DMG.');
const out = path.join(root, 'dist'),
  name = 'WoW-Addons-' + version + '-macos-universal.dmg';
fs.mkdirSync(out, { recursive: true });
fs.copyFileSync(path.join(source, files[0]), path.join(out, name));
const hash = crypto
  .createHash('sha256')
  .update(fs.readFileSync(path.join(out, name)))
  .digest('hex');
fs.writeFileSync(path.join(out, 'SHA256SUMS-macos.txt'), hash + '  ' + name + '\n');
console.log(name);

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

const appDir = path.join(root, 'target/universal-apple-darwin/release/bundle/macos');
const updates = fs.readdirSync(appDir).filter((f) => f.endsWith('.app.tar.gz'));
if (updates.length !== 1) throw new Error('Expected one universal updater archive.');
const updateName = 'WoW-Addons-' + version + '-macos-universal.app.tar.gz';
fs.copyFileSync(path.join(appDir, updates[0]), path.join(out, updateName));
fs.copyFileSync(path.join(appDir, updates[0] + '.sig'), path.join(out, updateName + '.sig'));

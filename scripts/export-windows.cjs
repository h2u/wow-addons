const fs = require('node:fs'),
  path = require('node:path'),
  crypto = require('node:crypto');
const root = path.resolve(__dirname, '..'),
  version = require('../package.json').version;
if (process.platform !== 'win32') throw new Error('Export Windows assets on Windows.');
const target = process.argv[2],
  base = path.join(root, 'target', ...(target ? [target] : []), 'release'),
  out = path.join(root, 'dist');
fs.mkdirSync(out, { recursive: true });
const name = 'WoW-Addons-' + version + '-windows-x64',
  assets = [];
function copy(source, name) {
  fs.copyFileSync(source, path.join(out, name));
  assets.push(name);
}
copy(path.join(base, 'wow-addons.exe'), name + '.exe');
const bundles = path.join(base, 'bundle', 'nsis'),
  installers = fs.readdirSync(bundles).filter((n) => n.endsWith('.exe'));
if (installers.length !== 1) throw new Error('Expected one NSIS installer.');
copy(path.join(bundles, installers[0]), name + '-setup.exe');
copy(path.join(bundles, installers[0] + '.sig'), name + '-setup.exe.sig');
require('node:child_process').execFileSync(
  process.execPath,
  [
    path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'),
    'signer',
    'sign',
    '--app-version',
    version,
    path.join(out, name + '.exe'),
  ],
  { stdio: 'inherit' },
);
assets.push(name + '.exe.sig');
fs.writeFileSync(
  path.join(out, 'SHA256SUMS.txt'),
  assets
    .map(
      (n) =>
        crypto
          .createHash('sha256')
          .update(fs.readFileSync(path.join(out, n)))
          .digest('hex') +
        '  ' +
        n,
    )
    .join('\n') + '\n',
);
console.log(assets.join('\n'));

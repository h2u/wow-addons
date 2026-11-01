const fs = require('node:fs'),
  path = require('node:path'),
  assert = require('node:assert/strict'),
  { execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
for (const dir of ['desktop', 'scripts', 'tests'])
  for (const name of fs.readdirSync(path.join(root, dir)).filter((n) => /\.(cjs|js)$/.test(n)))
    execFileSync(process.execPath, ['--check', path.join(root, dir, name)], { stdio: 'inherit' });
const pkg = require('../package.json'),
  version = pkg.version,
  lock = require('../package-lock.json'),
  config = require('../tauri.conf.json');
assert.match(version, /^\d+\.\d+\.\d+$/);
assert.equal(lock.version, version);
assert.equal(lock.packages[''].version, version);
assert.equal(config.version, version);
assert.ok(
  !Object.keys({ ...pkg.dependencies, ...pkg.devDependencies }).some((name) =>
    /electron/.test(name),
  ),
);
const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
assert.ok(cargo.includes('version = "' + version + '"'));
const cargoLock = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8');
assert.ok(cargoLock.includes('name = "wow-addons-core"\nversion = "' + version + '"'));
assert.ok(fs.readFileSync(path.join(root, 'desktop/index.html'), 'utf8').includes('bridge.js'));
console.log('Syntax, Tauri bridge and package versions checked.');

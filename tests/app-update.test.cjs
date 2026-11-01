const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { manifest } = require('../scripts/update-manifest.cjs');
test('manifest routes installed, portable and both macOS architectures to signed artifacts', () => {
  const names = ['windows-x64.exe', 'windows-x64-setup.exe', 'macos-universal.app.tar.gz'];
  const signatures = Object.fromEntries(
    names.map((n) => ['WoW-Addons-1.2.3-' + n + '.sig', 'signed-' + n]),
  );
  const result = manifest('1.2.3', 'h2u/wow-addons', signatures);
  assert.match(result.platforms['windows-x86_64-portable'].url, /windows-x64\.exe$/);
  assert.match(result.platforms['windows-x86_64'].url, /windows-x64-setup\.exe$/);
  assert.deepEqual(result.platforms['darwin-aarch64'], result.platforms['darwin-x86_64']);
  assert.throws(() => manifest('1.2.3', 'h2u/wow-addons', {}), /Missing signature/);
  assert.throws(() => manifest('1.2.3-beta', 'h2u/wow-addons', signatures), /stable/);
});
function setup(check) {
  const elements = new Map(),
    errors = [];
  const $ = (id) => {
    if (!elements.has(id)) elements.set(id, { hidden: true, dataset: {} });
    return elements.get(id);
  };
  const context = vm.createContext({
    $,
    working: false,
    window: { inventory: { checkAppUpdate: check, onAppUpdateProgress() {} } },
    error: (e) => errors.push(e),
  });
  const source = fs.readFileSync('desktop/renderer.js', 'utf8');
  vm.runInContext(source.slice(source.indexOf('let checkingApplicationUpdate = false;')), context);
  return {
    $,
    context,
    errors,
    run: (background) => vm.runInContext(`checkApplicationUpdate(${background})`, context),
  };
}
test('startup advertises new version without downloading or installing it', async () => {
  const app = setup(async () => ({ supported: true, available: true, version: '1.2.3' }));
  await app.run(true);
  assert.equal(app.$('app-update-banner').hidden, false);
  assert.equal(app.$('install-app-update').hidden, false);
  assert.match(app.$('app-update-status').textContent, /1.2.3/);
});
test('background network errors remain quiet; manual checks report errors and allow retry', async () => {
  const app = setup(async () => {
    throw Error('offline');
  });
  await app.run(true);
  assert.equal(app.errors.length, 0);
  await app.run(false);
  assert.equal(app.errors.length, 1);
  assert.equal(app.$('check-app-update').disabled, false);
});
test('up-to-date response hides the update action', async () => {
  const app = setup(async () => ({ supported: true, available: false }));
  await app.run(false);
  assert.equal(app.$('install-app-update').hidden, true);
  assert.match(app.$('app-update-status').textContent, /latest/);
});

test('background completion updates the disabled-state snapshot during an addon operation', async () => {
  const app = setup(async () => ({ supported: true, available: false }));
  // run() has disabled controls while an independent addon operation is active.
  vm.runInContext('working = true', app.context);
  app.$('check-app-update').dataset.preBusyDisabled = 'true';
  await app.run(true);
  assert.equal(app.$('check-app-update').dataset.preBusyDisabled, 'false');
});

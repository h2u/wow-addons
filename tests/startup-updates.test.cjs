const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('desktop/renderer.js', 'utf8');
const functions = source.slice(
  source.indexOf('async function maybeCheckStartupUpdates()'),
  source.indexOf('async function checkAddonUpdates(folder)'),
);
function setup({ enabled = true, games = ['a', 'b'], configured = true, fail } = {}) {
  const calls = [],
    notices = [];
  const context = vm.createContext({
    state: { checkUpdatesOnStartup: enabled, games: games.map((id) => ({ id })) },
    window: {
      inventory: {
        keyStatus: async () => {
          calls.push('key');
          return { configured };
        },
        checkUpdates: async (id) => {
          calls.push(id);
          if (id === fail) throw Error('offline');
          return { state: context.state, summary: { available: 2, errors: 0 } };
        },
      },
    },
    run: async (_, fn) => fn(),
    render() {},
    notice: (text) => notices.push(text),
  });
  vm.runInContext(functions, context);
  return {
    context,
    calls,
    notices,
    run: () => vm.runInContext('maybeCheckStartupUpdates()', context),
  };
}
test('startup checks respect preference, empty inventory, and missing credentials', async () => {
  for (const options of [{ enabled: false }, { games: [] }, { configured: false }]) {
    const app = setup(options);
    await app.run();
    assert.deepEqual(app.calls, options.configured === false ? ['key'] : []);
  }
});
test('startup checks every saved profile and reports updates without installing', async () => {
  const app = setup();
  await app.run();
  assert.deepEqual(app.calls, ['key', 'a', 'b']);
  assert.deepEqual(app.notices, ['4 addon updates available.']);
});
test('one failed profile does not skip remaining profiles', async () => {
  const app = setup({ fail: 'a' });
  await app.run();
  assert.deepEqual(app.calls, ['key', 'a', 'b']);
  assert.match(app.notices[0], /2 addon updates available.*checks failed/);
});

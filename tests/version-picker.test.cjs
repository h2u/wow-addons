const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function setup() {
  const elements = new Map();
  const $ = (id) => {
    if (!elements.has(id))
      elements.set(id, {
        value: '',
        disabled: false,
        hidden: false,
        replaceChildren(...children) {
          this.children = children;
          this.value = '';
        },
        showModal() {},
        close() {},
        addEventListener() {},
      });
    return elements.get(id);
  };
  const prepared = [],
    accepted = [];
  const context = vm.createContext({
    $,
    document: { createElement: () => ({}) },
    window: {
      inventory: {
        urlReleases: async () => ({
          name: 'Example',
          flavor: 'Retail',
          nextIndex: 50,
          files: [
            {
              id: 123,
              name: 'Older',
              channel: 'Stable',
              date: '2026-01-01',
              gameVersions: ['12'],
              url: 'https://www.curseforge.com/wow/addons/example/files/123',
            },
          ],
        }),
        prepareUrl: async (id, url) => {
          prepared.push({ id, url });
          return { token: 'plan' };
        },
        openAboutLink() {},
      },
    },
    run: async (_label, fn) => fn(),
    acceptPlan: (plan) => accepted.push(plan),
    showDownloadFallback() {},
    error() {},
  });
  const source = fs.readFileSync(require.resolve('../desktop/renderer.js'), 'utf8');
  vm.runInContext(
    source.slice(
      source.indexOf('let releasePicker = null;'),
      source.indexOf('let addonListSelection = null;'),
    ),
    context,
  );
  return { $, context, prepared, accepted };
}

test('URL picker defaults to latest and does not download before confirmation', async () => {
  const { $, context, prepared } = setup();
  await vm.runInContext(
    "openVersions({ id: 'game', url: 'https://www.curseforge.com/wow/addons/example/files/999' })",
    context,
  );
  assert.equal($('release-choice').value, 'latest');
  assert.equal($('install-release').disabled, false);
  assert.equal(prepared.length, 0);
  await $('install-release').onclick();
  assert.equal(prepared[0].url, 'https://www.curseforge.com/wow/addons/example');
});

test('selected older release survives pagination and prepares its exact URL', async () => {
  const { $, context, prepared, accepted } = setup();
  await vm.runInContext(
    "openVersions({ id: 'game', url: 'https://www.curseforge.com/wow/addons/example' })",
    context,
  );
  $('release-choice').value = '123';
  $('release-choice').onchange();
  await $('more-releases').onclick();
  assert.equal($('release-choice').value, '123');
  assert.equal($('release-choice').children.length, 2);
  await $('install-release').onclick();
  assert.equal(prepared[0].url, 'https://www.curseforge.com/wow/addons/example/files/123');
  assert.equal(accepted[0].token, 'plan');
});

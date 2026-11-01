const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs
  .readFileSync(path.join(__dirname, '../.github/workflows/pr-policy.yml'), 'utf8')
  .split('          script: |\n')[1]
  .split('\n')
  .map((line) => line.slice(12))
  .join('\n');
async function check(labels, title, body = '', releaseRequired = 'true') {
  const failures = [];
  const context = { repo: { owner: 'test', repo: 'test' }, issue: { number: 1 } };
  const github = {
    rest: {
      pulls: {
        get: async () => ({ data: { title, body, labels: labels.map((name) => ({ name })) } }),
      },
    },
  };
  await vm.runInNewContext('(async () => {' + source + '})()', {
    context,
    github,
    process: { env: { RELEASE_REQUIRED: releaseRequired } },
    core: { info: () => {}, setFailed: (message) => failures.push(message) },
  });
  return failures;
}
test('PR policy requires exactly one release label', async () => {
  for (const labels of [
    [],
    ['bug'],
    ['major', 'minor'],
    ['minor', 'chore'],
    ['major', 'chore'],
    ['major', 'minor', 'chore'],
  ])
    assert.equal((await check(labels, 'feat: change')).length, 1);
  assert.equal((await check(['chore', 'bug'], 'fix: change')).length, 0);
});
test('PR policy keeps release labels and conventional titles consistent', async () => {
  for (const [label, title] of [
    ['major', 'feat!: change'],
    ['major', 'feat(api)!: change'],
    ['minor', 'feat: change'],
    ['chore', 'chore: tools'],
    ['chore', 'fix(ui): layout'],
  ])
    assert.equal((await check([label], title)).length, 0);
  for (const [label, title] of [
    ['major', 'feat: change'],
    ['minor', 'feat!: change'],
    ['chore', 'feat: change'],
    ['chore', 'fix!: change'],
    ['minor', 'feat: '],
  ])
    assert.equal((await check([label], title)).length, 1);
});

test('PR policy rejects release overrides in the squash body', async () => {
  for (const body of ['Release-As: 9.0.0', 'BEGIN_COMMIT_OVERRIDE', 'BREAKING CHANGE: removed API'])
    assert.equal((await check(['chore'], 'chore: tools', body)).length, 1);
});

test('non-application PRs do not require labels; missing detection output fails closed', async () => {
  assert.equal((await check([], 'Update website', '', 'false')).length, 0);
  assert.equal((await check([], 'Update website', '', '')).length, 1);
});

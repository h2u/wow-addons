const test = require('node:test');
const assert = require('node:assert/strict');
const { classifyChanges, changedFiles } = require('../scripts/changes.cjs');
test('documentation and repository metadata do not build or release', () => {
  assert.deepEqual(
    classifyChanges([
      'README.md',
      'CONTRIBUTING.md',
      'docs/setup.md',
      '.gitignore',
      '.github/CODEOWNERS',
      'LICENSE',
    ]),
    { check: false, release: false },
  );
});
test('application, assets, dependencies and packaging run checks and release preparation', () => {
  for (const file of [
    'src/lib.rs',
    'desktop/styles.css',
    'desktop/assets/app-icon.png',
    'build/icon.ico',
    'build.rs',
    'Cargo.toml',
    'Cargo.lock',
    'package.json',
    'package-lock.json',
    'tauri.conf.json',
    'capabilities/main.json',
    'scripts/export-macos.cjs',
    'new-runtime.config',
  ])
    assert.deepEqual(classifyChanges([file]), { check: true, release: true }, file);
});
test('tests and CI/style tooling run checks without a release', () => {
  for (const file of [
    'tests/changes.test.cjs',
    '.github/workflows/build.yml',
    'scripts/changes.cjs',
    'scripts/smoke.cjs',
    'scripts/test-portable-update.cjs',
    '.prettierrc.json',
    'eslint.config.cjs',
    'rustfmt.toml',
    'scripts/draft-release.cjs',
  ])
    assert.deepEqual(classifyChanges([file]), { check: true, release: false }, file);
  assert.deepEqual(classifyChanges(['README.md', 'src/lib.rs']), { check: true, release: true });
});
test('PR detection covers the entire branch, pushes cover all pushed commits', () => {
  const base = 'a'.repeat(40),
    head = 'b'.repeat(40);
  for (const [name, event, expected] of [
    [
      'pull_request',
      { pull_request: { base: { sha: base }, head: { sha: head } } },
      [base + '...' + head],
    ],
    ['push', { before: base, after: head }, [base, head]],
  ]) {
    let args;
    assert.deepEqual(
      changedFiles(name, event, (a) => ((args = a), 'old.rs\0new.md\0')),
      ['old.rs', 'new.md'],
    );
    assert.deepEqual(args, ['diff', '--name-only', '--no-renames', '-z', ...expected]);
  }
  assert.equal(
    changedFiles('workflow_dispatch', {}, () => assert.fail()),
    null,
  );
  assert.throws(
    () => changedFiles('push', { before: base, after: 'invalid' }, () => assert.fail()),
    /SHA/,
  );
});
test('first push checks the complete tree and deleted files remain classified', () => {
  const head = 'b'.repeat(40);
  assert.deepEqual(
    changedFiles('push', { before: '0'.repeat(40), after: head }, (args) => {
      assert.deepEqual(args, ['ls-tree', '-r', '--name-only', '-z', head]);
      return 'src/removed.rs\0';
    }),
    ['src/removed.rs'],
  );
  assert.deepEqual(classifyChanges(['src/removed.rs', 'README.md']), {
    check: true,
    release: true,
  });
});

test('landing page and Pages workflow skip native checks and releases', () => {
  const site = [
    'site/index.html',
    'site/styles.css',
    'site/assets/icon.png',
    '.github/workflows/pages.yml',
  ];
  assert.deepEqual(classifyChanges(site), { check: false, release: false });
  assert.deepEqual(classifyChanges([...site, 'src/lib.rs']), { check: true, release: true });
  assert.deepEqual(classifyChanges([...site, '.github/workflows/build.yml']), {
    check: true,
    release: false,
  });
});

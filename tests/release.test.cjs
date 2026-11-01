const test = require('node:test');
const assert = require('node:assert/strict');
const { releaseTag, prepareRelease } = require('../scripts/manual-release.cjs');
test('initial release tags main without writing to the protected branch', () => {
  const sha = 'a'.repeat(40),
    calls = [];
  const gh = (args) => {
    calls.push(args);
    if (args[1] === 'list') return [];
    if (args[0] === 'api') return { sha };
    return '';
  };
  assert.equal(
    prepareRelease({ mode: 'initial', version: '0.1.0', ref: 'refs/heads/main', sha, gh }),
    'v0.1.0',
  );
  assert.deepEqual(
    calls.find((args) => args[1] === 'create'),
    [
      'release',
      'create',
      'v0.1.0',
      '--target',
      sha,
      '--draft',
      '--generate-notes',
      '--title',
      'v0.1.0',
    ],
  );
  assert.ok(calls.every((args) => !args.includes('push')));
  assert.throws(
    () => releaseTag({ mode: 'initial', version: '0.1.0', ref: 'refs/heads/feature' }),
    /from main/,
  );
});
test('release retry accepts only a valid existing draft', () => {
  assert.equal(
    prepareRelease({ mode: 'retry', tag: 'v0.1.0', gh: () => ({ isDraft: true }) }),
    'v0.1.0',
  );
  assert.throws(
    () => prepareRelease({ mode: 'retry', tag: 'v0.1.0', gh: () => ({ isDraft: false }) }),
    /draft/,
  );
  assert.throws(() => releaseTag({ mode: 'retry', tag: 'v0.1.0; anything' }), /vX.Y.Z/);
  assert.throws(
    () =>
      prepareRelease({
        mode: 'initial',
        version: '0.1.0',
        ref: 'refs/heads/main',
        gh: () => [{ tagName: 'v0.1.0' }],
      }),
    /already exists/,
  );
});
test('pre-existing tags cannot silently publish a different commit', () => {
  assert.throws(
    () =>
      prepareRelease({
        mode: 'initial',
        version: '0.1.0',
        ref: 'refs/heads/main',
        sha: 'a'.repeat(40),
        gh: (args) => (args[1] === 'list' ? [] : args[0] === 'api' ? { sha: 'b'.repeat(40) } : ''),
      }),
    /another commit/,
  );
});

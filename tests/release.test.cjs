const test = require('node:test');
const assert = require('node:assert/strict');
const { releaseTag, prepareRelease } = require('../scripts/manual-release.cjs');
const sha = 'a'.repeat(40);
function fakeGitHub({ tagged = false, target = sha, draft = true, resolved = sha } = {}) {
  const calls = [];
  const gh = (args) => {
    calls.push(args);
    if (args[0] === 'release' && args[1] === 'list') return [];
    if (args[0] === 'release' && args[1] === 'view')
      return { isDraft: draft, targetCommitish: target };
    if (args[1]?.includes('/git/matching-refs/'))
      return tagged ? [{ ref: 'refs/tags/v0.1.0' }] : [];
    if (args.includes('POST')) {
      tagged = true;
      return {};
    }
    if (args[1]?.includes('/commits/')) {
      if (!tagged) throw new Error('No commit found for SHA: v0.1.0 (HTTP 422)');
      return { sha: resolved };
    }
    return '';
  };
  return { gh, calls };
}
test('initial release explicitly creates the tag before creating a draft', () => {
  const { gh, calls } = fakeGitHub();
  assert.equal(
    prepareRelease({ mode: 'initial', version: '0.1.0', ref: 'refs/heads/main', sha, gh }),
    'v0.1.0',
  );
  const createTag = calls.findIndex((args) => args.includes('POST')),
    createDraft = calls.findIndex((args) => args[1] === 'create');
  assert.ok(createTag >= 0 && createTag < createDraft);
  assert.ok(calls[createTag].includes('ref=refs/tags/v0.1.0'));
  assert.ok(calls[createTag].includes('sha=' + sha));
  assert.ok(calls[createDraft].includes('--verify-tag'));
  assert.ok(calls.every((args) => !args.includes('push')));
  assert.throws(
    () => releaseTag({ mode: 'initial', version: '0.1.0', ref: 'refs/heads/feature' }),
    /from main/,
  );
});
test('retry restores the missing tag at the original draft SHA', () => {
  const { gh, calls } = fakeGitHub();
  assert.equal(prepareRelease({ mode: 'retry', tag: 'v0.1.0', sha: 'b'.repeat(40), gh }), 'v0.1.0');
  assert.ok(calls.find((args) => args.includes('POST')).includes('sha=' + sha));
  assert.ok(!calls.some((args) => args[1] === 'create'));
});
test('retry preserves an existing tag and rejects published releases', () => {
  const { gh, calls } = fakeGitHub({ tagged: true });
  prepareRelease({ mode: 'retry', tag: 'v0.1.0', gh });
  assert.ok(!calls.some((args) => args.includes('POST')));
  assert.throws(
    () => prepareRelease({ mode: 'retry', tag: 'v0.1.0', gh: fakeGitHub({ draft: false }).gh }),
    /draft/,
  );
  assert.throws(() => releaseTag({ mode: 'retry', tag: 'v0.1.0; anything' }), /vX.Y.Z/);
});
test('missing tags cannot be restored using a moving branch target', () => {
  const { gh, calls } = fakeGitHub({ target: 'main' });
  assert.throws(() => prepareRelease({ mode: 'retry', tag: 'v0.1.0', gh }), /immutable/);
  assert.ok(!calls.some((args) => args.includes('POST')));
});
test('conflicting tags fail before draft creation and are never moved', () => {
  const { gh, calls } = fakeGitHub({ tagged: true, resolved: 'b'.repeat(40) });
  assert.throws(
    () => prepareRelease({ mode: 'initial', version: '0.1.0', ref: 'refs/heads/main', sha, gh }),
    /another commit/,
  );
  assert.ok(!calls.some((args) => args.includes('POST') || args[1] === 'create'));
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

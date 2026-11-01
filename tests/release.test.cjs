const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'),
  os = require('node:os'),
  path = require('node:path');
const { syncDraft, nextVersion, MARKER } = require('../scripts/draft-release.cjs');
const { applyReleaseVersion } = require('../scripts/release-version.cjs');
test('a release batch takes the highest bump, including major before 1.0', () => {
  assert.equal(nextVersion('v0.1.0', ['chore', 'minor', 'major']), '1.0.0');
  assert.equal(nextVersion('v0.1.0', ['minor', 'chore']), '0.2.0');
  assert.equal(nextVersion('v0.1.0', ['chore', 'chore']), '0.1.1');
  assert.equal(nextVersion(null, ['major']), '0.1.0');
});
function fixture({
  labels = ['minor'],
  files = ['src/lib.rs'],
  pending = false,
  tagged = false,
  latest = true,
} = {}) {
  const writes = [],
    head = 'a'.repeat(40);
  const pr = {
    number: 12,
    title: 'feat: example',
    merged_at: '2026-11-01',
    base: { ref: 'main' },
    merge_commit_sha: head,
    labels: labels.map((name) => ({ name })),
    changed_files: files.length,
  };
  const published = { id: 1, tag_name: 'v0.1.0', draft: false };
  const existing = { id: 2, tag_name: 'v0.1.1', draft: true, body: MARKER };
  const api = {
    getBranch: async () => ({ data: { commit: { sha: latest ? head : 'b'.repeat(40) } } }),
    listReleases: 'releases',
    listPullRequestsAssociatedWithCommit: 'associated',
    getReleaseByTag: async () => {},
    getCommit: async () => {},
    createRelease: async (data) => {
      writes.push({ action: 'create', ...data });
      return { data: { id: 2 } };
    },
    updateRelease: async (data) => {
      writes.push({ action: 'update', ...data });
      return { data: { id: 2 } };
    },
  };
  const github = {
    rest: { repos: api, pulls: { get: async () => ({ data: pr }), listFiles: 'files' } },
    paginate: async (endpoint) => {
      if (endpoint === 'releases') return pending ? [published, existing] : [published];
      if (endpoint === 'associated') return [pr, pr];
      if (endpoint === 'files') return files.map((filename) => ({ filename }));
      throw new Error('Unexpected endpoint');
    },
  };
  return {
    writes,
    args: {
      github,
      context: { repo: { owner: 'test', repo: 'test' }, sha: head },
      git: (args) => (args[0] === 'tag' ? (tagged ? 'v0.2.0' : '') : head),
    },
  };
}
test('creates an accumulating draft without creating a tag or release PR', async () => {
  const f = fixture();
  const result = await syncDraft(f.args);
  assert.equal(result.tag, 'v0.2.0');
  assert.equal(f.writes.length, 1);
  assert.equal(f.writes[0].draft, true);
  assert.equal(f.writes[0].target_commitish, 'a'.repeat(40));
  assert.match(f.writes[0].body, /#12/);
});
test('later PRs update the same draft and major supersedes the previous version', async () => {
  const f = fixture({ labels: ['major'], pending: true });
  await syncDraft(f.args);
  assert.equal(f.writes[0].action, 'update');
  assert.equal(f.writes[0].release_id, 2);
  assert.equal(f.writes[0].tag_name, 'v1.0.0');
});
test('documentation-only batches and superseded main runs never create drafts', async () => {
  for (const options of [
    { files: ['README.md'] },
    { files: ['site/index.html', '.github/workflows/pages.yml'], labels: [] },
    { latest: false },
  ]) {
    const f = fixture(options);
    assert.ok((await syncDraft(f.args)).skipped);
    assert.equal(f.writes.length, 0);
  }
});
test('conflicting labels or existing tags stop draft preparation', async () => {
  await assert.rejects(
    () => syncDraft(fixture({ labels: ['major', 'minor'] }).args),
    /exactly one/,
  );
  await assert.rejects(() => syncDraft(fixture({ tagged: true }).args), /already exists/);
});
test('release builds stamp every app manifest while leaving dependency versions intact', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'wow-release-version-'));
  try {
    for (const name of [
      'package.json',
      'package-lock.json',
      'tauri.conf.json',
      'Cargo.toml',
      'Cargo.lock',
    ])
      fs.copyFileSync(path.join(__dirname, '..', name), path.join(root, name));
    assert.equal(applyReleaseVersion(root, 'v2.3.4'), '2.3.4');
    const lock = JSON.parse(fs.readFileSync(path.join(root, 'package-lock.json')));
    assert.equal(lock.version, '2.3.4');
    assert.equal(lock.packages[''].version, '2.3.4');
    assert.equal(
      lock.packages['node_modules/eslint'].version,
      require('../package-lock.json').packages['node_modules/eslint'].version,
    );
    assert.match(
      fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8'),
      /name = "wow-addons-core"\nversion = "2.3.4"/,
    );
    assert.match(
      fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8'),
      /\[package\][\s\S]*?version = "2.3.4"/,
    );
    assert.equal(JSON.parse(fs.readFileSync(path.join(root, 'tauri.conf.json'))).version, '2.3.4');
    assert.throws(() => applyReleaseVersion(root, 'v1.2.3;bad'), /tag/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('published release selection pins a tag SHA and rejects drafts, immutable or off-main tags', async () => {
  const { selectRelease } = require('../scripts/release-target.cjs');
  function args(options = {}) {
    return {
      tag: 'v0.2.0',
      context: { repo: { owner: 'test', repo: 'test' } },
      github: {
        rest: {
          repos: {
            getReleaseByTag: async () => ({
              data: { draft: !!options.draft, immutable: !!options.immutable },
            }),
            getCommit: async () => ({ data: { sha: 'a'.repeat(40) } }),
            compareCommitsWithBasehead: async () => ({
              data: { status: options.status || 'ahead' },
            }),
          },
        },
      },
    };
  }
  assert.deepEqual(await selectRelease(args()), { tag: 'v0.2.0', sha: 'a'.repeat(40) });
  for (const options of [{ draft: true }, { immutable: true }, { status: 'diverged' }])
    await assert.rejects(() => selectRelease(args(options)));
  await assert.rejects(() => selectRelease({ ...args(), tag: 'main' }), /tag/);
});

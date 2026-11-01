const { execFileSync } = require('node:child_process');
const { classifyChanges } = require('./changes.cjs');
const MARKER = '<!-- wow-addons:pending-release -->';
const VERSION = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
function nextVersion(baseline, labels, initial = '0.1.0') {
  if (!baseline) return initial;
  const match = VERSION.exec(baseline);
  if (!match) throw new Error('Invalid stable release tag.');
  const [major, minor, patch] = match.slice(1).map(Number);
  if (labels.includes('major')) return major + 1 + '.0.0';
  if (labels.includes('minor')) return major + '.' + (minor + 1) + '.0';
  return major + '.' + minor + '.' + (patch + 1);
}
function sortReleases(a, b) {
  const av = a.tag_name.slice(1).split('.').map(Number),
    bv = b.tag_name.slice(1).split('.').map(Number);
  return bv[0] - av[0] || bv[1] - av[1] || bv[2] - av[2];
}
async function syncDraft({
  github,
  context,
  git = (args) => execFileSync('git', args, { encoding: 'utf8' }),
  initial = require('../package.json').version,
}) {
  const repo = context.repo;
  const head = (await github.rest.repos.getBranch({ ...repo, branch: 'main' })).data.commit.sha;
  if (head !== context.sha) return { skipped: 'A newer main commit will update the draft.' };
  const releases = await github.paginate(github.rest.repos.listReleases, {
    ...repo,
    per_page: 100,
  });
  const baseline = releases
    .filter((r) => !r.draft && !r.prerelease && VERSION.test(r.tag_name))
    .sort(sortReleases)[0];
  const commits = git(['rev-list', '--reverse', baseline ? baseline.tag_name + '..' + head : head])
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  const commitSet = new Set(commits),
    prs = new Map();
  for (const commit_sha of commits) {
    const associated = await github.paginate(
      github.rest.repos.listPullRequestsAssociatedWithCommit,
      { ...repo, commit_sha, per_page: 100 },
    );
    for (const pr of associated) {
      if (pr.merged_at && pr.base.ref === 'main' && commitSet.has(pr.merge_commit_sha))
        prs.set(pr.number, pr);
    }
  }
  const notes = [],
    labels = [];
  for (const number of [...prs.keys()].sort((a, b) => a - b)) {
    const pr = (await github.rest.pulls.get({ ...repo, pull_number: number })).data;
    const files = await github.paginate(github.rest.pulls.listFiles, {
      ...repo,
      pull_number: number,
      per_page: 100,
    });
    const relevant =
      files.length < pr.changed_files ||
      classifyChanges(
        files.flatMap((f) => [f.filename, ...(f.previous_filename ? [f.previous_filename] : [])]),
      ).release;
    notes.push('- ' + pr.title.replace(/[\r\n]/g, ' ') + ' (#' + number + ')');
    if (!relevant) continue;
    const selected = pr.labels
      .map((l) => l.name)
      .filter((l) => ['major', 'minor', 'chore'].includes(l));
    if (selected.length !== 1)
      throw new Error('PR #' + number + ' must have exactly one release label.');
    labels.push(selected[0]);
  }
  // Bootstrap history can predate the PR policy; subsequent releases require merged PRs.
  if (!labels.length && baseline)
    return { skipped: 'No application changes since the published release.' };
  if (!commits.length) return { skipped: 'No new commits.' };
  const version = nextVersion(baseline?.tag_name, labels, initial),
    tag = 'v' + version;
  const pending = releases.filter((r) => r.draft && r.body?.includes(MARKER));
  if (pending.length > 1)
    throw new Error('Multiple managed drafts exist; resolve them before preparing a release.');
  const existing = pending[0];
  if (releases.some((r) => r.id !== existing?.id && r.tag_name === tag))
    throw new Error('Another release already uses ' + tag);
  const tags = git(['tag', '--list', tag]).trim();
  if (tags) throw new Error('The pending tag already exists; existing tags are never moved.');
  const body =
    MARKER +
    '\n\n## Changes\n\n' +
    (notes.join('\n') || '- Initial release') +
    '\n\nPublish this draft to build Windows and macOS installers. Assets appear after CI succeeds.\n';
  const data = {
    ...repo,
    tag_name: tag,
    target_commitish: head,
    name: tag,
    body,
    draft: true,
    prerelease: false,
  };
  const result = existing
    ? await github.rest.repos.updateRelease({ ...data, release_id: existing.id })
    : await github.rest.repos.createRelease(data);
  return { tag, id: result.data.id, url: result.data.html_url };
}
module.exports = { syncDraft, nextVersion, MARKER };

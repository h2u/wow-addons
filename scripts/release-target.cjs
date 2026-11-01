async function selectRelease({ github, context, tag }) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag || ''))
    throw new Error('Expected a vX.Y.Z tag.');
  const { data: release } = await github.rest.repos.getReleaseByTag({ ...context.repo, tag });
  if (release.draft || release.prerelease)
    throw new Error('Publish a stable release before building assets.');
  if (release.immutable)
    throw new Error('Disable immutable releases for publish-first asset builds.');
  const { data: commit } = await github.rest.repos.getCommit({ ...context.repo, ref: tag });
  const { data: comparison } = await github.rest.repos.compareCommitsWithBasehead({
    ...context.repo,
    basehead: commit.sha + '...main',
  });
  if (!['ahead', 'identical'].includes(comparison.status))
    throw new Error('Release source must be a commit on main.');
  return { tag, sha: commit.sha };
}
module.exports = { selectRelease };

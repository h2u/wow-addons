const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
function releaseTag({ mode, version, tag, ref }) {
  if (mode === 'initial') {
    if (ref !== 'refs/heads/main') throw new Error('Create the first release from main.');
    if (!SEMVER.test(version)) throw new Error('Invalid application version.');
    return 'v' + version;
  }
  if (mode === 'retry' && tag?.startsWith('v') && SEMVER.test(tag.slice(1))) return tag;
  throw new Error('Choose initial or retry with a vX.Y.Z tag.');
}
function prepareRelease({ mode, version, tag, ref, sha, gh }) {
  const selected = releaseTag({ mode, version, tag, ref });
  if (mode === 'retry') {
    const release = gh(['release', 'view', selected, '--json', 'isDraft']);
    if (!release.isDraft) throw new Error('Retry is allowed only for an existing draft.');
    return selected;
  }
  const published = gh([
    'release',
    'list',
    '--limit',
    '1',
    '--exclude-drafts',
    '--exclude-pre-releases',
    '--json',
    'tagName',
  ]);
  if (published.length)
    throw new Error('The first stable release already exists. Use a release PR.');
  const existing = gh(['release', 'list', '--limit', '100', '--json', 'tagName,isDraft']);
  if (existing.some((r) => r.tagName === selected))
    throw new Error('This release already exists. Use retry with ' + selected + '.');
  if (!/^[a-f0-9]{40}$/i.test(sha || '')) throw new Error('A checked-out commit SHA is required.');
  gh([
    'release',
    'create',
    selected,
    '--target',
    sha,
    '--draft',
    '--generate-notes',
    '--title',
    selected,
  ]);
  const resolved = gh(['api', 'repos/{owner}/{repo}/commits/' + selected]);
  if (resolved.sha !== sha)
    throw new Error('The tag points to another commit. The release remains a draft.');
  return selected;
}
if (require.main === module) {
  const gh = (args) => {
    const output = execFileSync('gh', args, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    try {
      return JSON.parse(output);
    } catch {
      return output.trim();
    }
  };
  try {
    const tag = prepareRelease({
      mode: process.env.RELEASE_MODE,
      version: require('../package.json').version,
      tag: process.env.REQUESTED_TAG,
      ref: process.env.GITHUB_REF,
      sha: process.env.GITHUB_SHA,
      gh,
    });
    fs.appendFileSync(process.env.GITHUB_OUTPUT, 'created=true\ntag=' + tag + '\n');
    console.log('Prepared draft ' + tag);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
module.exports = { releaseTag, prepareRelease };

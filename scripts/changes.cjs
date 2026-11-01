const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
function classifyChanges(files) {
  let check = false,
    release = false;
  for (const file of files) {
    if (
      /^(?:site\/|\.github\/workflows\/pages\.yml$|docs\/|(?:README|CONTRIBUTING|CHANGELOG)(?:\.[^/]*)?$|LICENSE$|\.gitignore$|\.gitattributes$|\.editorconfig$|\.github\/(?:CODEOWNERS|ISSUE_TEMPLATE\/|PULL_REQUEST_TEMPLATE))/i.test(
        file,
      )
    )
      continue;
    check = true;
    if (
      /^(?:tests\/|\.github\/|rustfmt\.toml$|eslint\.config\.cjs$|\.prettier|scripts\/(?:check|test|test-portable-update|smoke|downgrade-smoke|native-driver|zip-fixture|changes|draft-release|manual-release)\.cjs$)/.test(
        file,
      )
    )
      continue;
    release = true;
  }
  return { check, release };
}
function changedFiles(eventName, event, git) {
  if (eventName === 'workflow_dispatch') return null;
  const base = eventName === 'pull_request' ? event.pull_request.base.sha : event.before;
  const head = eventName === 'pull_request' ? event.pull_request.head.sha : event.after;
  if (!/^[a-f0-9]{40}$/i.test(head || '')) throw new Error('Invalid head SHA.');
  if (!base || /^0+$/.test(base))
    return git(['ls-tree', '-r', '--name-only', '-z', head]).split('\0').filter(Boolean);
  if (!/^[a-f0-9]{40}$/i.test(base)) throw new Error('Invalid base SHA.');
  return git([
    'diff',
    '--name-only',
    '--no-renames',
    '-z',
    eventName === 'pull_request' ? base + '...' + head : base,
    ...(eventName === 'pull_request' ? [] : [head]),
  ])
    .split('\0')
    .filter(Boolean);
}
if (require.main === module) {
  const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const files = changedFiles(process.env.GITHUB_EVENT_NAME, event, (args) =>
    execFileSync('git', args, { encoding: 'utf8' }),
  );
  const result = files === null ? { check: true, release: true } : classifyChanges(files);
  fs.appendFileSync(
    process.env.GITHUB_OUTPUT,
    Object.entries(result)
      .map(([key, value]) => key + '=' + value + '\n')
      .join(''),
  );
  console.log(JSON.stringify({ ...result, files: files?.length ?? 'manual' }));
}
module.exports = { classifyChanges, changedFiles };

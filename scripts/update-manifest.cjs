// Build a single update manifest only after all platform assets are available.
const fs = require('node:fs');
const path = require('node:path');
function manifest(version, repository, signatures, notes = '') {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Expected a stable release version.');
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error('Invalid repository.');
  const base = `https://github.com/${repository}/releases/download/v${version}/`;
  const name = `WoW-Addons-${version}`;
  const files = {
    'windows-x86_64': name + '-windows-x64-setup.exe',
    'windows-x86_64-portable': name + '-windows-x64.exe',
    'darwin-x86_64': name + '-macos-universal.app.tar.gz',
    'darwin-aarch64': name + '-macos-universal.app.tar.gz',
  };
  const platforms = Object.fromEntries(
    Object.entries(files).map(([target, file]) => {
      const signature = signatures[file + '.sig']?.trim();
      if (!signature) throw new Error('Missing signature for ' + file);
      return [target, { url: base + file, signature }];
    }),
  );
  return { version, notes, platforms };
}
if (require.main === module) {
  const directory = process.argv[2] || 'dist';
  const signatures = Object.fromEntries(
    fs
      .readdirSync(directory)
      .filter((n) => n.endsWith('.sig'))
      .map((n) => [n, fs.readFileSync(path.join(directory, n), 'utf8')]),
  );
  const result = manifest(
    process.env.RELEASE_TAG?.replace(/^v/, ''),
    process.env.GH_REPO,
    signatures,
  );
  fs.writeFileSync(path.join(directory, 'latest.json'), JSON.stringify(result, null, 2) + '\n');
}
module.exports = { manifest };

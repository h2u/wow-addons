const fs = require('node:fs');
const path = require('node:path');
function applyReleaseVersion(root, tag) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag || ''))
    throw new Error('Expected a vX.Y.Z release tag.');
  const version = tag.slice(1),
    writes = new Map();
  for (const name of ['package.json', 'package-lock.json', 'tauri.conf.json']) {
    const data = JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));
    data.version = version;
    if (name === 'package-lock.json') data.packages[''].version = version;
    writes.set(name, JSON.stringify(data, null, 2) + '\n');
  }
  for (const [name, pattern] of [
    ['Cargo.toml', /(\[package\][\s\S]*?\nversion = ")[^"]+(")/],
    ['Cargo.lock', /(\[\[package\]\]\nname = "wow-addons-core"\nversion = ")[^"]+(")/],
  ]) {
    const original = fs.readFileSync(path.join(root, name), 'utf8');
    if (!pattern.test(original)) throw new Error('Missing application version in ' + name);
    writes.set(
      name,
      original.replace(pattern, (_, before, after) => before + version + after),
    );
  }
  for (const [name, contents] of writes) fs.writeFileSync(path.join(root, name), contents);
  return version;
}
if (require.main === module)
  console.log(
    'Building release ' +
      applyReleaseVersion(path.resolve(__dirname, '..'), process.env.RELEASE_TAG),
  );
module.exports = { applyReleaseVersion };

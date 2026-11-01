const assert = require('node:assert/strict');
const version = require('../package.json').version;
assert.match(version, /^\d+\.\d+\.\d+$/);
assert.equal(
  process.env.RELEASE_TAG,
  'v' + version,
  'Release tag must match the checked out app version.',
);
console.log('Release ' + version);

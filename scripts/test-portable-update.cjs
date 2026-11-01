// Native helper integration test. Uses only isolated copies and synthetic processes.
const fs = require('node:fs/promises'),
  path = require('node:path'),
  os = require('node:os');
const crypto = require('node:crypto'),
  assert = require('node:assert/strict');
const { spawn, execFileSync } = require('node:child_process');
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
async function exists(p) {
  return fs.access(p).then(
    () => true,
    () => false,
  );
}
(async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'wow portable update spaces '));
  const source = process.env.WOW_ADDONS_BINARY || path.resolve('target/debug/wow-addons.exe');
  let fixture = process.env.WOW_ADDONS_UPDATE_FIXTURE;
  if (!fixture) {
    fixture = path.join(root, 'startup-fixture.exe');
    execFileSync('rustc', ['tests/fixtures/update-ready.rs', '-o', fixture], { stdio: 'inherit' });
  }
  if (!fixture)
    throw Error('Set WOW_ADDONS_UPDATE_FIXTURE to a startup-acknowledging native fixture.');
  for (const failure of [false, true]) {
    const folder = path.join(root, failure ? 'rollback' : 'success');
    await fs.mkdir(folder);
    const target = path.join(folder, 'WoW Addons portable.exe');
    const dir = path.join(folder, '.wow-addons-update-' + crypto.randomUUID());
    await fs.mkdir(dir);
    await fs.writeFile(target, 'previous executable');
    await fs.copyFile(source, path.join(dir, 'updater.exe'));
    const loader = path.join(path.dirname(source), 'WebView2Loader.dll');
    if (await exists(loader)) await fs.copyFile(loader, path.join(dir, 'WebView2Loader.dll'));
    const bytes = failure ? Buffer.from('invalid PE file') : await fs.readFile(fixture);
    await fs.writeFile(path.join(dir, 'candidate.exe'), bytes);
    const parent = spawn(process.execPath, ['-e', 'setTimeout(()=>process.exit(0),60000)'], {
      windowsHide: true,
      stdio: 'ignore',
    });
    const helper = spawn(
      path.join(dir, 'updater.exe'),
      [
        '--apply-portable-update',
        target,
        String(parent.pid),
        crypto.createHash('sha256').update(bytes).digest('hex'),
      ],
      {
        windowsHide: true,
        stdio: 'inherit',
        env: { ...process.env, WOW_ADDONS_TEST_DATA: folder },
      },
    );
    try {
      for (let i = 0; i < 300 && !(await exists(path.join(dir, 'ready'))); i++) await pause(100);
      assert.equal(
        await exists(path.join(dir, 'ready')),
        true,
        'helper acquired parent process handle',
      );
      assert.equal(
        await fs.readFile(target, 'utf8'),
        'previous executable',
        'original is untouched while parent runs',
      );
      parent.kill();
      await new Promise((resolve) => parent.once('exit', resolve));
      for (let i = 0; i < 300 && !(await exists(path.join(dir, 'finished'))); i++) await pause(100);
      assert.equal(await exists(path.join(dir, 'finished')), true);
      if (failure) assert.equal(await fs.readFile(target, 'utf8'), 'previous executable');
      else assert.deepEqual(await fs.readFile(target), bytes);
      console.log(
        failure
          ? 'Native rollback restored previous executable.'
          : 'Native portable replacement and startup acknowledgement passed at path with spaces.',
      );
    } finally {
      if (parent.exitCode === null && parent.signalCode === null) parent.kill();
      if (helper.exitCode === null) helper.kill();
    }
  }
  console.log('Isolated test artifacts: ' + root);
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

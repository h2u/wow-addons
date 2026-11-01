// Optional live Windows test. Uses a copy of the old encrypted setting to exercise migration.
const fs = require('node:fs/promises'),
  path = require('node:path'),
  assert = require('node:assert/strict'),
  driver = require('./native-driver.cjs');
let app;
(async () => {
  if (process.platform !== 'win32') {
    console.log('Skipped: Windows live test.');
    return;
  }
  const profile = await driver.profile('wow-tauri-live-'),
    old = path.join(process.env.APPDATA, 'wow-addons');
  try {
    await fs.copyFile(path.join(old, 'credentials.json'), path.join(profile, 'credentials.json'));
    await fs.copyFile(path.join(old, 'Local State'), path.join(profile, 'Local State'));
  } catch (e) {
    if (e.code === 'ENOENT') {
      console.log(
        'Skipped: no legacy key available. Enter a key manually into a temporary Tauri profile for live checks.',
      );
      return;
    }
    throw e;
  }
  app = await driver.launch(profile);
  const { evaluate, wait } = app;
  assert.equal(
    (await evaluate('await window.inventory.keyStatus()')).configured,
    true,
    'The encrypted key must migrate without the old runtime.',
  );
  const client = path.join(profile, '_forever_');
  await fs.mkdir(path.join(client, 'Interface', 'AddOns'), { recursive: true });
  await evaluate('state=await window.inventory.add(' + JSON.stringify([client]) + ');render()');
  const plan = await evaluate(
    'await window.inventory.prepareUrl(selected().id,"https://www.curseforge.com/wow/addons/bagnon")',
  );
  if (plan.blocked) throw new Error('Browser download required: ' + plan.downloadPage);
  await evaluate(
    'state=(await window.inventory.installZip(' + JSON.stringify(plan.token) + ')).state;render()',
  );
  await evaluate('await checkAddonUpdates("Bagnon")');
  assert.equal(await evaluate('state.updates[selected().id].Bagnon.status'), 'current');
  await evaluate('await openVersions({id:selected().id,folder:"Bagnon"})');
  const listing = await evaluate(
    '({files:releasePicker.files,installed:releasePicker.installedFileId})',
  );
  const older = listing.files.find((f) => f.channel === 'Stable' && f.id !== listing.installed);
  assert.ok(older);
  await evaluate(
    'document.getElementById("release-choice").value=' +
      JSON.stringify(String(older.id)) +
      ';document.getElementById("release-choice").dispatchEvent(new Event("change"));document.getElementById("install-release").click()',
  );
  await wait('!working && document.getElementById("import-dialog").open');
  assert.equal(await evaluate('pendingImport.source.fileId'), older.id);
  await evaluate('document.getElementById("confirm-import").click()');
  await wait('!working && state.packages[selected().id].some(p=>p.fileId===' + older.id + ')');
  await evaluate('await checkAddonUpdates("Bagnon")');
  assert.equal(await evaluate('state.updates[selected().id].Bagnon.status'), 'available');
  console.log(
    JSON.stringify({
      nativeKeyMigration: true,
      realApi: true,
      downloadAndInstall: true,
      downgrade: older.name,
      updates: true,
      client,
    }),
  );
})()
  .catch((e) => {
    console.error(e);
    process.exitCode = 1;
  })
  .finally(async () => {
    if (app) {
      await app.evaluate('await window.inventory.clearApiKey()').catch(() => {});
      await app.close();
    }
  });

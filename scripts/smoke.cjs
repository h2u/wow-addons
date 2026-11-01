// Runs against a debug build using synthetic clients; never touches a real game.
const fs = require('node:fs/promises'),
  path = require('node:path'),
  assert = require('node:assert/strict'),
  driver = require('./native-driver.cjs'),
  zip = require('./zip-fixture.cjs');
let app, profile;
(async () => {
  profile = await driver.profile();
  app = await driver.launch(profile);
  const { evaluate, wait } = app;
  const client = path.join(profile, '_forever_');
  await fs.mkdir(path.join(client, 'Interface', 'AddOns'), { recursive: true });
  assert.equal(await evaluate('document.getElementById("empty-state").hidden'), false);
  await evaluate(
    'window.testErrors=[];window.addEventListener("error",e=>testErrors.push(e.message));window.addEventListener("unhandledrejection",e=>testErrors.push(String(e.reason)));',
  );
  await evaluate(
    'document.getElementById("empty-add").click();document.getElementById("manage-add").click();document.getElementById("game-path").value=' +
      JSON.stringify(client) +
      ';document.getElementById("detect-games").click()',
  );
  await wait('!working && document.querySelectorAll("#detected-games input").length===1');
  await evaluate('document.getElementById("confirm-add").click()');
  await wait('!working && !document.getElementById("library").hidden');
  assert.equal(await evaluate('document.getElementById("install-from-url").disabled'), false);
  const stage = await evaluate(
    'document.getElementById("rescan").click();({disabled:document.getElementById("rescan").disabled,overlay:!document.getElementById("table-loading").hidden})',
  );
  assert.equal(stage.disabled, true);
  assert.equal(stage.overlay, true);
  await wait('!working');
  const input = path.join(profile, 'addon.zip'),
    files = (version) => ({
      'Bagnon/Bagnon.toc': '## Interface: 16001\n## Title: Bagnon\n## Version: ' + version,
      'BagBrother/BagBrother.toc': '## Interface: 16001\n## Title: BagBrother',
      'Bagnon/main.lua': version,
    });
  await fs.writeFile(input, zip(files('1.0.0')));
  await evaluate('importArchive(' + JSON.stringify(input) + ')');
  await wait('!working && selected().addons.length===2');
  assert.equal(
    await evaluate('document.querySelectorAll("#table-body tr[data-package]").length'),
    1,
  );
  await evaluate('showDetails("Bagnon")');
  await evaluate(
    'document.getElementById("source-url").value="https://www.curseforge.com/wow/addons/bagnon";document.getElementById("save-binding").click()',
  );
  await wait('!working && document.getElementById("binding-status").textContent.includes("saved")');
  await evaluate('document.getElementById("details-dialog").close()');
  await fs.writeFile(input, zip(files('2.0.0')));
  await evaluate('importArchive(' + JSON.stringify(input) + ')');
  await wait('document.getElementById("import-dialog").open');
  await evaluate('document.getElementById("cancel-import").click()');
  assert.equal(
    await fs.readFile(path.join(client, 'Interface/AddOns/Bagnon/main.lua'), 'utf8'),
    '1.0.0',
  );
  await evaluate('importArchive(' + JSON.stringify(input) + ')');
  await wait('document.getElementById("import-dialog").open');
  await evaluate('document.getElementById("confirm-import").click()');
  await wait('!working && selected().addons.some(a=>a.version==="2.0.0")');
  await evaluate('document.getElementById("settings").click()');
  await wait('document.getElementById("api-key-status").textContent.length>0');
  await evaluate(
    'document.getElementById("api-key").value="tauri-test-not-a-real-key";document.getElementById("save-api-key").click()',
  );
  await wait('!working && document.getElementById("api-key-status").textContent.includes("saved")');
  assert.equal((await evaluate('await window.inventory.keyStatus()')).saved, true);
  const saved = await fs.readFile(path.join(profile, 'inventory.json'), 'utf8');
  await app.close();
  app = await driver.launch(profile);
  assert.equal((await app.evaluate('await window.inventory.keyStatus()')).configured, true);
  assert.equal(await fs.readFile(path.join(profile, 'inventory.json'), 'utf8'), saved);
  await app.evaluate(
    'await window.inventory.clearApiKey();document.querySelector("[data-actions=Bagnon]").click();document.querySelector("[data-action=remove]").click()',
  );
  assert.equal(
    await app.evaluate('document.querySelectorAll("#remove-folders .import-package").length'),
    2,
  );
  await app.evaluate('document.getElementById("confirm-remove").click()');
  await app.wait('!working && selected().addons.length===0');
  assert.ok(
    (await fs.readdir(path.join(profile, 'backups', await app.evaluate('selected().id')))).some(
      (f) => f.includes('removed'),
    ),
  );
  console.log(
    JSON.stringify({
      nativeTauri: true,
      zipInstall: true,
      cancelReinstall: true,
      reinstall: true,
      groupRemoval: true,
      keySaveClear: true,
      keySurvivesRestart: true,
      cachedStartup: true,
      profile,
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

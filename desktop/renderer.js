document.addEventListener('contextmenu', (event) => event.preventDefault());
const $ = (id) => document.getElementById(id);
const escapeHtml = (value) =>
  String(value ?? '').replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c],
  );
const uiIcon = (name) => '<span class="ph-icon ph-' + name + '" aria-hidden="true"></span>';
const labels = {
  addon: 'Addon',
  version: 'Installed version',
  compatibility: 'Compatibility',
  source: 'Source',
  folder: 'Folder',
  interface: 'Interface',
  toc: 'TOC file',
};
let downloadFallbackUrl = null;
let pendingImport = null,
  actionTarget = null,
  removalTarget = null;
let state,
  candidates = [],
  detailFolder = null,
  working = false,
  cachePath = '';
const icons = new Map();
const gameIcons = new Map();
const expandedGroups = new Set();
const groupsFor = (g) =>
  window.addonGroups.groupAddons(
    g.addons,
    state.bindings[g.id] ?? {},
    state.packages?.[g.id] ?? [],
  );
const expansionKey = (g, a) => g.id + '|' + a.folder;
function displayPath(value) {
  const text = String(value),
    prefix = String.fromCharCode(92, 92, 63, 92);
  if (!text.startsWith(prefix)) return text;
  const rest = text.slice(4);
  return rest.startsWith('UNC\\') ? '\\\\' + rest.slice(4) : rest;
}
function selected() {
  return state?.games.find((g) => g.id === state.selectedId) ?? state?.games[0];
}
function binding(g, a) {
  const bindings = state.bindings[g.id] ?? {};
  if (bindings[a.folder]) return bindings[a.folder];
  const group = groupsFor(g).find((group) => group.members.some((m) => m.folder === a.folder));
  return group?.members.map((m) => bindings[m.folder]).find(Boolean);
}
function source(g, a) {
  const members = a.members ?? [a];
  return members.some((m) => binding(g, m)?.url)
    ? 'CurseForge'
    : members.some((m) => m.wagoId)
      ? 'Wago'
      : 'Not linked';
}
function error(e) {
  const text = String(e.message ?? e).replace(
    /^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/,
    '',
  );
  const dialog = document.querySelector('dialog[open]');
  if (dialog) {
    let notice = dialog.querySelector('.dialog-error');
    if (!notice) {
      notice = document.createElement('div');
      notice.className = 'dialog-error';
      notice.setAttribute('role', 'alert');
      dialog.append(notice);
    }
    notice.textContent = text;
  } else {
    showToast('error', text);
  }
}
async function run(label, fn) {
  if (working) return;
  document.querySelectorAll('.dialog-error').forEach((el) => el.remove());
  dismissToast('notice');
  working = true;
  setStage(label);
  $('table-loading').hidden = false;
  $('rescan').classList.add('is-loading');
  $('table-body').inert = true;
  $('busy-text').textContent = label;
  $('busy').hidden = false;
  $('error').hidden = true;
  document.querySelectorAll('button,select,input').forEach((b) => {
    b.dataset.preBusyDisabled = String(b.disabled);
    b.disabled = true;
  });
  try {
    await fn();
  } catch (e) {
    error(e);
  } finally {
    working = false;
    $('table-loading').hidden = true;
    $('rescan').classList.remove('is-loading');
    $('table-body').inert = false;
    $('busy').hidden = true;
    document.querySelectorAll('button,select,input').forEach((b) => {
      if (b.dataset.preBusyDisabled !== undefined) {
        b.disabled = b.dataset.preBusyDisabled === 'true';
        delete b.dataset.preBusyDisabled;
      }
    });
    $('confirm-add').disabled = !document.querySelector('#detected-games input:checked');
  }
}
function setStage(text) {
  $('busy-text').textContent = text;
  $('table-loading-text').textContent = text;
  if ($('url-dialog').open) $('url-status').textContent = text;
}
window.inventory.onProgress(setStage);
let noticeTimer;
function dismissToast(id) {
  $(id).hidden = true;
  if (id === 'notice') clearTimeout(noticeTimer);
}
function armNotice() {
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => dismissToast('notice'), 5000);
}
function showToast(id, text) {
  const toast = $(id);
  toast.querySelector('.toast-message').textContent =
    id === 'notice' ? text.replace(/ Backup: [\s\S]*$/, ' Backup saved.') : text;
  toast.title = text;
  toast.hidden = false;
  if (id === 'notice') armNotice();
}
function notice(text) {
  showToast('notice', text);
}
document
  .querySelectorAll('[data-dismiss]')
  .forEach((button) => (button.onclick = () => dismissToast(button.dataset.dismiss)));
$('notice').onmouseenter = () => clearTimeout(noticeTimer);
$('notice').onmouseleave = armNotice;
$('notice').addEventListener('focusin', () => clearTimeout(noticeTimer));
$('notice').addEventListener('focusout', armNotice);
function hash(name) {
  return [...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 4;
}
function updateFor(g, a) {
  const root = groupsFor(g).find((group) => group.members.some((m) => m.folder === a.folder));
  const info = state.updates?.[g.id]?.[root?.folder];
  return info && info.installedVersion && info.installedVersion !== root.version
    ? { ...info, status: 'unchecked' }
    : info;
}
function cell(g, a, key) {
  if (key === 'addon')
    return (
      '<div class="addon-cell">' +
      (a.members?.length > 1
        ? '<button class="expand-group" data-expand="' +
          escapeHtml(a.folder) +
          '" aria-label="Show modules for ' +
          escapeHtml(a.name) +
          '" aria-expanded="' +
          expandedGroups.has(expansionKey(g, a)) +
          '">' +
          uiIcon(expandedGroups.has(expansionKey(g, a)) ? 'caret-down' : 'caret-right') +
          '</button>'
        : '<span class="expand-spacer"></span>') +
      '<span class="addon-icon" data-tone="' +
      hash(a.folder) +
      '" data-icon-folder="' +
      escapeHtml(a.folder) +
      '">' +
      uiIcon('puzzle-piece') +
      '</span><div><div class="addon-name" title="' +
      escapeHtml(a.name) +
      '">' +
      escapeHtml(a.name) +
      '</div><div class="addon-sub">' +
      (a.members?.length > 1 ? a.members.length + ' folders · ' : '') +
      escapeHtml(a.folder) +
      '</div></div></div>'
    );
  if (key === 'version') {
    const version = a.version ? escapeHtml(a.version) : '<span class="muted">Not declared</span>';
    const info = a.members ? updateFor(g, a) : null;
    if (!info) return version;
    if (info.status === 'available')
      return (
        '<div>' +
        version +
        '</div><button class="update-badge" data-update="' +
        escapeHtml(a.folder) +
        '" title="' +
        escapeHtml(info.reason) +
        '">' +
        uiIcon('arrows-clockwise') +
        'Update to ' +
        escapeHtml(info.latestVersion) +
        '</button>'
      );
    const label =
      {
        current: 'Up to date',
        ahead: 'Ahead of stable',
        unverified: 'Latest: ' + info.latestVersion + ' · verify',
        error: 'Check failed',
        unlinked: 'Not linked',
        unchecked: 'Check again',
      }[info.status] || '';
    return (
      '<div>' +
      version +
      '</div><span class="version-status ' +
      escapeHtml(info.status) +
      '" title="' +
      escapeHtml(info.reason) +
      '">' +
      escapeHtml(label) +
      '</span>'
    );
  }
  if (key === 'compatibility')
    return (
      '<span class="badge ' +
      (a.compatibility === 'Review compatibility' ? 'warn' : '') +
      '">' +
      escapeHtml(a.compatibility) +
      '</span>'
    );
  if (key === 'source') {
    const s = source(g, a);
    return (
      '<span class="' + (s === 'Not linked' ? 'muted' : 'source') + '">' + escapeHtml(s) + '</span>'
    );
  }
  return '<span class="mono">' + escapeHtml(a[key] || '—') + '</span>';
}
function render() {
  if (!state) return;
  const g = selected(),
    filter = $('filter').value.toLocaleLowerCase();
  const installButton = $('install-from-url');
  if (working && installButton.dataset.preBusyDisabled !== undefined)
    installButton.dataset.preBusyDisabled = String(!g);
  else installButton.disabled = !g;
  const updateCount = g
    ? groupsFor(g).filter((a) => updateFor(g, a)?.status === 'available').length
    : 0;
  $('update-all').hidden = !updateCount;
  $('update-all').querySelector('span:last-child').textContent = 'Update all (' + updateCount + ')';
  $('library').hidden = !g;
  $('empty-state').hidden = !!g;
  $('heading-count').textContent = g ? groupsFor(g).length : '';
  $('page-description').textContent = g
    ? 'Everything installed in your ' + g.installation.name + ' client.'
    : 'A little order for your next adventure.';
  $('game-tabs').innerHTML = state.games
    .map((game) => {
      const active = game.id === g?.id;
      return (
        '<button class="client-tab ' +
        (active ? 'active' : '') +
        '" role="tab" aria-selected="' +
        active +
        '" tabindex="' +
        (active ? '0' : '-1') +
        '" data-game-id="' +
        escapeHtml(game.id) +
        '" aria-label="' +
        escapeHtml(game.installation.name) +
        '" title="' +
        escapeHtml(game.installation.name) +
        '"><span class="client-icon" data-client-icon="' +
        escapeHtml(game.id) +
        '">' +
        uiIcon('game-controller') +
        '</span>' +
        (active
          ? '<span class="client-name">' + escapeHtml(game.installation.name) + '</span>'
          : '') +
        '</button>'
      );
    })
    .join('');
  hydrateGameIcons();
  if (!g) return;
  const keys = state.columnOrder.filter((k) => state.columns.includes(k));
  $('table-head').innerHTML =
    '<tr>' +
    keys
      .map(
        (k) =>
          '<th><button data-sort="' +
          k +
          '">' +
          labels[k] +
          (state.sort.key === k
            ? uiIcon(state.sort.direction === 1 ? 'arrow-up' : 'arrow-down')
            : '') +
          '</button></th>',
      )
      .join('') +
    '<th class="actions-heading" aria-label="Actions"></th></tr>';
  const value = (a) =>
    state.sort.key === 'addon'
      ? a.name
      : state.sort.key === 'source'
        ? source(g, a)
        : (a[state.sort.key] ?? '');
  const allGroups = groupsFor(g);
  const matches = (a) =>
    [a.name, a.folder, a.version, source(g, a)].some((s) =>
      String(s).toLocaleLowerCase().includes(filter),
    );
  const addons = allGroups
    .filter((a) => a.members.some(matches))
    .sort(
      (a, b) =>
        String(value(a))
          .replace(/^[^\p{L}\p{N}]+/u, '')
          .localeCompare(String(value(b)).replace(/^[^\p{L}\p{N}]+/u, ''), undefined, {
            numeric: true,
            ignorePunctuation: true,
          }) * state.sort.direction,
    );
  const row = (a, child = false) =>
    '<tr tabindex="0" ' +
    (child ? 'class="module-row" data-child="true"' : 'data-package="true"') +
    ' data-folder="' +
    escapeHtml(a.folder) +
    '" aria-label="Details for ' +
    escapeHtml(a.name) +
    '">' +
    keys.map((k) => '<td>' + cell(g, a, k) + '</td>').join('') +
    '<td class="row-actions"><button data-actions="' +
    escapeHtml(a.folder) +
    '" aria-label="Actions for ' +
    escapeHtml(a.name) +
    '">' +
    uiIcon('dots-three-vertical') +
    '</button></td></tr>';
  $('table-body').innerHTML = addons
    .map(
      (a) =>
        row(a) +
        (expandedGroups.has(expansionKey(g, a))
          ? a.members
              .filter((m) => m.folder !== a.folder)
              .map((m) => row(m, true))
              .join('')
          : ''),
    )
    .join('');
  $('no-results').hidden = addons.length > 0;
  $('no-results').textContent = filter
    ? 'No addons match your filter.'
    : 'No addons installed. Drop a ZIP or use Install from URL.';
  const linked = allGroups.filter((a) => source(g, a) !== 'Not linked').length;
  $('inventory-summary').innerHTML =
    '<strong>' +
    addons.length +
    (filter ? ' of ' + allGroups.length : '') +
    ' addons</strong> <span> · ' +
    g.addons.length +
    ' folders · ' +
    linked +
    ' with a source</span>';
  $('scan-time').textContent = 'Folders checked ' + new Date(g.scannedAt * 1000).toLocaleString();
  $('folder-path').textContent = displayPath(g.installation.addonsPath);
  $('folder-path').title = g.installation.addonsPath;
  $('warnings').hidden = !g.warnings.length;
  $('warnings-summary').textContent = g.warnings.length + ' scan notes';
  $('warnings-list').innerHTML = g.warnings.map((w) => '<li>' + escapeHtml(w) + '</li>').join('');
  hydrateIcons(g);
}
async function hydrateGameIcons() {
  for (const g of state.games) {
    if (!gameIcons.has(g.id))
      gameIcons.set(
        g.id,
        window.inventory.clientIcon(g.id).catch(() => null),
      );
    const data = await gameIcons.get(g.id);
    if (data)
      for (const el of document.querySelectorAll('[data-client-icon]'))
        if (el.dataset.clientIcon === g.id) {
          const img = document.createElement('img');
          img.src = data;
          img.alt = '';
          el.replaceChildren(img);
        }
  }
}
async function hydrateIcons(g) {
  for (const a of groupsFor(g)) {
    const key = g.id + '|' + a.folder + '|' + a.iconPath;
    if (!icons.has(key)) {
      icons.set(key, null);
      try {
        icons.set(key, await window.inventory.icon(g.id, a.folder));
      } catch {}
    }
    if (selected()?.id !== g.id) return;
    const data = icons.get(key);
    if (data)
      for (const el of document.querySelectorAll('[data-icon-folder]')) {
        if (el.dataset.iconFolder === a.folder) {
          const img = document.createElement('img');
          img.src = data;
          img.alt = '';
          el.replaceChildren(img);
        }
      }
  }
}
function openAdd() {
  candidates = [];
  $('detected-games').innerHTML = '';
  $('detect-status').textContent = '';
  $('game-path').value = '';
  $('confirm-add').disabled = true;
  $('add-dialog').showModal();
  $('game-path').focus();
}
async function detect() {
  await run('Finding game clients…', async () => {
    candidates = await window.inventory.discover($('game-path').value);
    $('detected-games').innerHTML = candidates
      .map(
        (g, index) =>
          '<label class="detected-row"><input type="checkbox" value="' +
          index +
          '" checked><div><strong>' +
          escapeHtml(g.name) +
          '</strong><small>' +
          escapeHtml(displayPath(g.root)) +
          '</small></div></label>',
      )
      .join('');
    $('detect-status').textContent =
      candidates.length + ' client' + (candidates.length === 1 ? '' : 's') + ' found';
  });
}
function renderColumns() {
  $('column-options').innerHTML = state.columnOrder
    .map(
      (key, index) =>
        '<div class="column-option"><label><input type="checkbox" data-column="' +
        key +
        '" ' +
        (state.columns.includes(key) ? 'checked' : '') +
        ' ' +
        (key === 'addon' ? 'disabled' : '') +
        '>' +
        labels[key] +
        '</label><button data-move="' +
        key +
        '" data-dir="-1" ' +
        (index === 0 ? 'disabled' : '') +
        ' aria-label="Move ' +
        labels[key] +
        ' up">' +
        uiIcon('arrow-up') +
        '</button><button data-move="' +
        key +
        '" data-dir="1" ' +
        (index === state.columnOrder.length - 1 ? 'disabled' : '') +
        ' aria-label="Move ' +
        labels[key] +
        ' down">' +
        uiIcon('arrow-down') +
        '</button></div>',
    )
    .join('');
}
function renderManage() {
  $('managed-games').innerHTML = state.games.length
    ? state.games
        .map(
          (g) =>
            '<div class="managed-row"><div><strong>' +
            escapeHtml(g.installation.name) +
            '</strong><small>' +
            escapeHtml(displayPath(g.installation.root)) +
            '</small><small>' +
            g.addons.length +
            ' addon folders</small></div><button class="subtle danger" data-remove="' +
            escapeHtml(g.id) +
            '">' +
            uiIcon('trash') +
            '<span>Remove profile</span></button></div>',
        )
        .join('')
    : '<p>No game profiles yet.</p>';
  $('cache-location').textContent = 'Stored locally on this computer.';
  $('cache-location').title = cachePath;
}
function showDetails(folder) {
  const g = selected(),
    a = g.addons.find((a) => a.folder === folder);
  if (!a) return;
  detailFolder = folder;
  $('detail-name').textContent = a.name;
  $('detail-avatar').innerHTML = uiIcon('puzzle-piece');
  loadDetailIcon(g.id, folder);
  const fields = [
    ['Installed version', a.version || 'Not declared'],
    ['Folder', a.folder],
    ['Selected TOC', a.toc || 'Not found'],
    ['Interface', a.interface || 'Not declared'],
    ['Compatibility', a.compatibility],
    ['CurseForge project ID', a.curseProjectId || 'Not declared'],
    ['Wago project ID', a.wagoId || 'Not declared'],
    ['Dependencies', a.dependencies.join(', ') || 'None declared'],
  ];
  const group = groupsFor(g).find((group) => group.members.some((m) => m.folder === folder));
  $('detail-content').innerHTML =
    '<dl class="detail-grid">' +
    fields
      .map(([k, v]) => '<dt>' + escapeHtml(k) + '</dt><dd>' + escapeHtml(v) + '</dd>')
      .join('') +
    '</dl>' +
    (group?.members.length > 1
      ? '<h3 class="package-heading">Package folders (' +
        group.members.length +
        ')</h3><div class="package-members">' +
        group.members
          .map(
            (m) =>
              '<button class="package-member" data-member="' +
              escapeHtml(m.folder) +
              '"><span>' +
              escapeHtml(m.folder) +
              '</span><span class="muted">' +
              escapeHtml(m.version || 'Module') +
              '</span></button>',
          )
          .join('') +
        '</div>'
      : '');
  const groupLink = group?.members.map((m) => binding(g, m)?.url).find(Boolean);
  $('source-url').value = groupLink || '';
  if (!groupLink && group?.members.some((m) => m.curseProjectId)) resolveDetailSource(g.id, folder);
  $('binding-status').textContent = '';
  $('icon-status').textContent = '';
  if (!$('details-dialog').open) $('details-dialog').showModal();
}
function openManage() {
  renderManage();
  $('manage-dialog').showModal();
}
async function renderKeyStatus() {
  const status = await window.inventory.keyStatus();
  const disabled = !status.configured && !status.saved;
  const clear = $('clear-api-key');
  if (working && clear.dataset.preBusyDisabled !== undefined)
    clear.dataset.preBusyDisabled = String(disabled);
  else clear.disabled = disabled;
  $('api-key').placeholder = status.configured
    ? 'API key is saved · enter a new key to replace it'
    : 'Paste your CurseForge API key';
  $('api-key-status').textContent = status.configured
    ? status.saved
      ? 'Key saved on this device.'
      : 'Key available for this session.'
    : status.saved
      ? 'Saved key cannot be unlocked. Enter and save it again.'
      : 'No API key configured.';
}
async function renderAbout() {
  const about = await window.inventory.about();
  $('app-version').textContent = 'v' + about.version;
}
$('settings').onclick = () => {
  $('settings-dialog').showModal();
  renderKeyStatus().catch(error);
  renderAbout().catch(error);
};
$('open-repository').onclick = () => window.inventory.openAboutLink('repository').catch(error);
$('open-coffee').onclick = () => window.inventory.openAboutLink('support').catch(error);
$('add-game').onclick = openManage;
$('empty-add').onclick = openManage;
$('manage-add').onclick = () => {
  $('manage-dialog').close();
  openAdd();
};
$('detect-games').onclick = detect;
$('game-path').onkeydown = (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    detect();
  }
};
for (const [id, kind] of [
  ['browse-folder', 'folder'],
  ['browse-file', 'file'],
])
  $(id).onclick = async () => {
    await run('Choose a game…', async () => {
      const p = await window.inventory.browse(kind);
      if (p) $('game-path').value = p;
    });
    if ($('game-path').value) await detect();
  };
$('detected-games').onchange = () => {
  $('confirm-add').disabled = !document.querySelector('#detected-games input:checked');
};
$('confirm-add').onclick = () =>
  run('Scanning addon folders…', async () => {
    const roots = [...document.querySelectorAll('#detected-games input:checked')].map(
      (el) => candidates[Number(el.value)].root,
    );
    state = await window.inventory.add(roots);
    $('add-dialog').close();
    $('filter').value = '';
    render();
  });
function selectGame(id) {
  if (id === selected()?.id) return;
  return run('Opening library…', async () => {
    state = await window.inventory.select(id);
    $('filter').value = '';
    render();
    document.querySelector('[data-game-id="' + CSS.escapeHtml(id) + '"]')?.focus();
  });
}
$('game-tabs').onclick = (e) => {
  const button = e.target.closest('[data-game-id]');
  if (button) selectGame(button.dataset.gameId);
};
$('game-tabs').onkeydown = (e) => {
  const buttons = [...document.querySelectorAll('[data-game-id]')],
    index = buttons.findIndex((b) => b === document.activeElement);
  let next;
  if (e.key === 'ArrowRight') next = (index + 1) % buttons.length;
  else if (e.key === 'ArrowLeft') next = (index - 1 + buttons.length) % buttons.length;
  else if (e.key === 'Home') next = 0;
  else if (e.key === 'End') next = buttons.length - 1;
  else return;
  e.preventDefault();
  selectGame(buttons[next].dataset.gameId);
};
$('filter').oninput = render;
$('rescan').onclick = () =>
  run('Scanning addon folders…', async () => {
    state = await window.inventory.rescan(selected().id);
    icons.clear();
    render();
    const report = await window.inventory.report(selected().id);
    if (report) {
      notice(
        'Rescan complete. ' +
          (report.downloaded ?? 0) +
          ' icons downloaded, ' +
          (report.local ?? 0) +
          ' local icons, ' +
          (report.cached ?? 0) +
          ' cached.' +
          (report.missing?.length
            ? ' No icon found for ' + report.missing.length + ' addons.'
            : '') +
          (report.errors?.length ? ' ' + report.errors.join('; ') : ''),
      );
    }
  });
$('columns').onclick = () => {
  renderColumns();
  $('columns-dialog').showModal();
};
$('column-options').onchange = (e) => {
  if (!e.target.dataset.column) return;
  const columns = [...document.querySelectorAll('[data-column]:checked')].map(
    (el) => el.dataset.column,
  );
  run('Saving columns…', async () => {
    state = await window.inventory.preferences({ columns });
    render();
    renderColumns();
  });
};
$('column-options').onclick = (e) => {
  const b = e.target.closest('[data-move]');
  if (!b) return;
  const order = [...state.columnOrder],
    index = order.indexOf(b.dataset.move),
    to = index + Number(b.dataset.dir);
  if (to < 0 || to >= order.length) return;
  [order[index], order[to]] = [order[to], order[index]];
  run('Saving columns…', async () => {
    state = await window.inventory.preferences({ columnOrder: order });
    render();
    renderColumns();
  });
};
$('reset-columns').onclick = () =>
  run('Saving columns…', async () => {
    state = await window.inventory.preferences({
      columns: ['addon', 'version'],
      columnOrder: Object.keys(labels),
    });
    render();
    renderColumns();
  });
$('table-head').onclick = (e) => {
  const b = e.target.closest('[data-sort]');
  if (!b) return;
  run('Saving view…', async () => {
    state = await window.inventory.preferences({
      sort: {
        key: b.dataset.sort,
        direction: state.sort.key === b.dataset.sort ? -state.sort.direction : 1,
      },
    });
    render();
  });
};
$('table-body').onclick = (e) => {
  if (working) return;
  const update = e.target.closest('[data-update]');
  if (update) {
    updateAddon(update.dataset.update);
    return;
  }
  const action = e.target.closest('[data-actions]');
  if (action) {
    openAddonMenu(action);
    return;
  }
  const button = e.target.closest('[data-expand]');
  if (button) {
    const key = selected().id + '|' + button.dataset.expand;
    expandedGroups.has(key) ? expandedGroups.delete(key) : expandedGroups.add(key);
    render();
    return;
  }
  const row = e.target.closest('[data-folder]');
  if (row) showDetails(row.dataset.folder);
};
$('detail-content').onclick = (e) => {
  const button = e.target.closest('[data-member]');
  if (button) showDetails(button.dataset.member);
};
$('table-body').onkeydown = (e) => {
  if (e.key === 'Enter' && !e.target.closest('[data-expand],[data-actions],[data-update]')) {
    const row = e.target.closest('[data-folder]');
    if (row) showDetails(row.dataset.folder);
  }
};
$('managed-games').onclick = (e) => {
  const b = e.target.closest('[data-remove]');
  if (!b) return;
  run('Removing profile…', async () => {
    state = await window.inventory.remove(b.dataset.remove);
    renderManage();
    render();
  });
};
$('get-icon').onclick = () =>
  run('Refreshing addon icon…', async () => {
    const id = selected().id,
      folder = detailFolder;
    const result = await window.inventory.getIcon(id, folder);
    icons.clear();
    render();
    await loadDetailIcon(id, folder);
    $('icon-status').textContent = result.errors.length
      ? result.errors.join('; ')
      : result.downloaded || result.local || result.cached
        ? 'Icon refreshed and saved locally.'
        : 'Icon not found. Configure a project URL and API key in Settings.';
  });
$('clear-api-key').onclick = () =>
  run('Clearing API key…', async () => {
    await window.inventory.clearApiKey();
    $('api-key').value = '';
    await renderKeyStatus();
  });
$('save-api-key').onclick = () =>
  run('Saving API key…', async () => {
    await window.inventory.saveApiKey($('api-key').value);
    $('api-key').value = '';
    await renderKeyStatus();
  });
$('save-binding').onclick = () =>
  run('Saving source link…', async () => {
    state = await window.inventory.bind(selected().id, detailFolder, $('source-url').value);
    icons.clear();
    $('binding-status').textContent = 'Link saved locally';
    render();
  });

async function resolveDetailSource(id, folder) {
  try {
    state = await window.inventory.resolveSource(id);
    if (selected()?.id !== id || detailFolder !== folder) return;
    const group = groupsFor(selected()).find((g) => g.members.some((a) => a.folder === folder));
    const url = group?.members.map((a) => binding(selected(), a)?.url).find(Boolean);
    if (url && !$('source-url').value) {
      $('source-url').value = url;
      $('binding-status').textContent = 'Verified CurseForge project';
    }
    render();
  } catch (e) {
    if (detailFolder === folder) $('binding-status').textContent = e.message;
  }
}
function openAddonMenu(button) {
  actionTarget = { id: selected().id, folder: button.dataset.actions };
  const menu = $('addon-menu');
  const group = groupsFor(selected()).find((g) =>
    g.members.some((a) => a.folder === button.dataset.actions),
  );
  menu.querySelector('[data-action=source]').disabled = !group.members.some(
    (a) => a.curseProjectId || binding(selected(), a)?.url,
  );
  menu.querySelector('[data-action=downgrade]').disabled = !group.members.some(
    (a) => binding(selected(), a)?.url,
  );
  menu.showPopover();
  const info = updateFor(selected(), group);
  menu.querySelector('[data-action=update]').hidden =
    !info || !['available', 'unverified'].includes(info.status);
  menu.querySelector('[data-update-label]').textContent =
    info?.status === 'unverified' ? 'Install latest' : 'Update';
  menu.querySelector('[data-action=download]').hidden = !info?.downloadPage;
  const rect = button.getBoundingClientRect();
  menu.style.left =
    Math.max(8, Math.min(rect.right - menu.offsetWidth, innerWidth - menu.offsetWidth - 8)) + 'px';
  menu.style.top = Math.min(rect.bottom + 4, innerHeight - menu.offsetHeight - 8) + 'px';
}
$('addon-menu').onclick = async (e) => {
  const button = e.target.closest('[data-action]');
  if (!button || !actionTarget || working) return;
  const target = { ...actionTarget };
  $('addon-menu').hidePopover();
  if (button.dataset.action === 'check') return checkAddonUpdates(target.folder);
  if (button.dataset.action === 'downgrade') return openVersions(target);
  if (button.dataset.action === 'update') return updateAddon(target.folder);
  if (button.dataset.action === 'download') {
    const g = state.games.find((g) => g.id === target.id),
      a = g.addons.find((a) => a.folder === target.folder);
    return window.inventory.openDownload(updateFor(g, a).downloadPage).catch(error);
  }
  if (button.dataset.action === 'folder')
    return run('Opening addon folder…', () =>
      window.inventory.showFolder(target.id, target.folder),
    );
  if (button.dataset.action === 'source')
    return run('Opening CurseForge…', async () => {
      state = await window.inventory.viewSource(target.id, target.folder);
      render();
    });
  if (button.dataset.action === 'reinstall') {
    const file = await window.inventory.chooseZip();
    if (file) await importArchive(file, target.id, target.folder);
    return;
  }
  if (button.dataset.action === 'remove') askRemove(target);
};
async function importArchive(file, id = selected()?.id, expectedFolder) {
  if (!id) {
    error('Add and select a game client first.');
    return;
  }
  let plan;
  await run('Checking addon ZIP…', async () => {
    plan = await window.inventory.inspectZip(id, file);
  });
  if (!plan) return;
  if (expectedFolder) {
    const g = state.games.find((g) => g.id === id),
      group = groupsFor(g).find((g) => g.members.some((a) => a.folder === expectedFolder));
    if (!plan.packages.some((p) => group.members.some((a) => a.folder === p.folder))) {
      await window.inventory.cancelImport(plan.token);
      error('This ZIP does not contain the selected addon.');
      return;
    }
  }
  acceptPlan(plan);
}
function acceptPlan(plan) {
  $('pick-import-version').hidden = !plan.source?.url;
  if (plan.packages.some((p) => p.exists) || (plan.skipped ?? []).some((p) => p.exists)) {
    pendingImport = plan;
    $('import-packages').innerHTML =
      plan.packages
        .map(
          (p) =>
            '<div class="import-package"><strong>' +
            escapeHtml(p.folder) +
            '</strong><span>' +
            escapeHtml(p.version || '') +
            ' · ' +
            (p.exists ? 'Already installed' : 'New') +
            '</span></div>',
        )
        .join('') +
      (plan.skipped ?? [])
        .map(
          (p) =>
            '<div class="import-package"><strong>' +
            escapeHtml(p.folder) +
            '</strong><span>Other game client · ' +
            (p.exists ? 'will be backed up and removed' : 'skipped') +
            '</span></div>',
        )
        .join('');
    $('import-dialog').showModal();
  } else installPlan(plan.token);
}

async function installPlan(token) {
  await run('Installing addon…', async () => {
    const result = await window.inventory.installZip(token);
    state = result.state;
    icons.clear();
    render();
    notice(
      'Installed ' +
        result.installed.installed.join(', ') +
        ((result.installed.skipped ?? []).length
          ? '. Skipped ' + result.installed.skipped.length + ' modules for other clients.'
          : '') +
        '. Backup: ' +
        displayPath(result.installed.backupPath),
    );
  });
}
$('pick-import-version').onclick = async () => {
  const plan = pendingImport;
  if (!plan?.source?.url) return;
  const target = { id: selected().id, url: plan.source.url };
  $('import-dialog').close();
  await openVersions(target);
};
$('confirm-import').onclick = () => {
  const token = pendingImport?.token;
  pendingImport = null;
  $('import-dialog').close();
  if (token) installPlan(token);
};
$('cancel-import').onclick = () => {
  $('import-dialog').close();
};
$('import-dialog').addEventListener('close', () => {
  if (pendingImport) {
    window.inventory.cancelImport(pendingImport.token).catch(error);
    pendingImport = null;
  }
});
$('cancel-remove').onclick = () => {
  $('remove-dialog').close();
};
$('confirm-remove').onclick = () => {
  const target = removalTarget;
  $('remove-dialog').close();
  if (!target) return;
  run('Removing addon…', async () => {
    const result = await window.inventory.removeAddon(target.id, target.folder);
    state = result.state;
    render();
    notice('Addon removed. Backup: ' + displayPath(result.removed.backupPath));
  });
};
window.inventory.onDrop(async (event) => {
  if (event.type === 'leave') {
    $('drop-overlay').hidden = true;
    return;
  }
  if (working) return;
  if (event.type === 'enter' || event.type === 'over') {
    $('drop-target').textContent = selected()
      ? 'Install into ' + selected().installation.name
      : 'Add a game client first';
    $('drop-overlay').hidden = false;
    return;
  }
  if (event.type !== 'drop') return;
  $('drop-overlay').hidden = true;
  const files = event.paths || [];
  if (files.length !== 1) {
    error('Drop one addon ZIP at a time.');
    return;
  }
  if (document.querySelector('dialog[open]')) {
    error('Close this dialog before importing an addon ZIP.');
    return;
  }
  await importArchive(files[0]);
});

async function loadDetailIcon(id, folder) {
  try {
    const data = await window.inventory.icon(id, folder);
    if (selected()?.id !== id || detailFolder !== folder) return;
    const target = $('detail-avatar');
    target.replaceChildren();
    if (data) {
      const img = document.createElement('img');
      img.src = data;
      img.alt = '';
      target.append(img);
    } else target.innerHTML = uiIcon('puzzle-piece');
  } catch {}
}
function askRemove(target) {
  const g = state.games.find((g) => g.id === target.id),
    group = groupsFor(g).find((g) => g.members.some((a) => a.folder === target.folder));
  if (!group) return;
  if ($('details-dialog').open) $('details-dialog').close();
  removalTarget = target;
  $('remove-title').textContent = 'Remove ' + group.name + '?';
  $('remove-folders').innerHTML = group.members
    .map((a) => '<div class="import-package">' + escapeHtml(a.folder) + '</div>')
    .join('');
  $('remove-dialog').showModal();
}
$('detail-show-folder').onclick = () =>
  window.inventory.showFolder(selected().id, detailFolder).catch(error);
$('detail-remove').onclick = () => askRemove({ id: selected().id, folder: detailFolder });
$('detail-close').onclick = () => {
  $('details-dialog').close();
};
function openUrlDialog() {
  if (!selected()) return;
  downloadFallbackUrl = null;
  $('install-url').value = '';
  $('url-status').textContent = '';
  $('url-fallback').hidden = true;
  $('url-client').textContent =
    'Install into ' +
    selected().installation.name +
    '. The latest compatible stable release will be selected.';
  $('url-dialog').showModal();
  $('install-url').focus();
}
function showDownloadFallback(result) {
  downloadFallbackUrl = result.downloadPage;
  $('install-url').value = result.source?.url || result.downloadPage;
  $('url-status').textContent = result.reason || 'Open the download page and import the ZIP.';
  $('url-fallback').hidden = false;
  $('url-client').textContent = 'Download for ' + selected().installation.name;
  if (!$('url-dialog').open) $('url-dialog').showModal();
}
$('install-from-url').onclick = openUrlDialog;
$('confirm-url').onclick = async () => {
  const target = { id: selected().id, url: $('install-url').value.trim() };
  $('url-dialog').close();
  await openVersions(target);
};
$('install-url').onkeydown = (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    $('confirm-url').click();
  }
};
$('url-open-page').onclick = () =>
  window.inventory.openDownload(downloadFallbackUrl || $('install-url').value).catch(error);
$('url-import-zip').onclick = async () => {
  const file = await window.inventory.chooseZip();
  if (file) {
    $('url-dialog').close();
    await importArchive(file);
  }
};
async function checkAddonUpdates(folder) {
  await run('Checking addon updates…', async () => {
    const result = await window.inventory.checkUpdates(selected().id, folder);
    state = result.state;
    render();
    notice(
      'Checked ' +
        result.summary.checked +
        ' addons: ' +
        result.summary.available +
        ' updates available.' +
        (result.summary.unverified
          ? ' ' + result.summary.unverified + ' installed builds need verification.'
          : '') +
        (result.summary.errors ? ' ' + result.summary.errors + ' checks failed.' : ''),
    );
  });
}
$('check-updates').onclick = () => checkAddonUpdates();
async function updateAddon(folder) {
  let blocked;
  await run('Updating addon…', async () => {
    const g = selected(),
      a = g.addons.find((a) => a.folder === folder);
    if (!a) throw new Error('Addon was not found.');
    const info = updateFor(g, a),
      url = info?.downloadPage || binding(g, a)?.url;
    if (!url) throw new Error('Link this addon to a CurseForge project first.');
    const plan = await window.inventory.prepareUrl(g.id, url);
    if (plan.blocked) {
      blocked = plan;
      return;
    }
    const result = await window.inventory.installZip(plan.token);
    state = result.state;
    icons.clear();
    render();
    notice('Updated ' + result.installed.installed.join(', ') + '. Backup saved.');
  });
  if (blocked) showDownloadFallback(blocked);
}
$('update-all').onclick = async () => {
  let blocked;
  await run('Updating addons…', async () => {
    const g = selected(),
      todo = groupsFor(g)
        .filter((a) => updateFor(g, a)?.status === 'available')
        .map((a) => ({ folder: a.folder, url: updateFor(g, a).downloadPage }));
    let installed = 0;
    const errors = [];
    for (const item of todo) {
      try {
        const plan = await window.inventory.prepareUrl(g.id, item.url);
        if (plan.blocked) {
          blocked ??= plan;
          errors.push(item.folder + ': download in browser');
          continue;
        }
        const result = await window.inventory.installZip(plan.token);
        state = result.state;
        installed++;
      } catch (e) {
        errors.push(
          item.folder +
            ': ' +
            String(e.message ?? e).replace(
              /^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/,
              '',
            ),
        );
      }
    }
    icons.clear();
    render();
    notice('Updated ' + installed + ' addons.' + (errors.length ? ' ' + errors.join('; ') : ''));
  });
  if (blocked) showDownloadFallback(blocked);
};

window.addEventListener('error', (e) => error(e.error || e.message));
window.addEventListener('unhandledrejection', (e) => error(e.reason));
(async () => {
  try {
    const loaded = await window.inventory.load();
    cachePath = loaded.cachePath;
    if (loaded.error) {
      $('fatal').textContent = loaded.error + '\n' + cachePath;
      $('fatal').hidden = false;
      document.querySelectorAll('button,select,input').forEach((b) => (b.disabled = true));
      return;
    }
    state = loaded.state;
    render();
  } catch (e) {
    error(e);
  }
})();

let releasePicker = null;
async function openVersions(target) {
  releasePicker = { ...target, files: [], nextIndex: 0, installedFileId: null };
  $('release-choice').replaceChildren();
  $('versions-summary').textContent = 'Loading compatible releases…';
  $('release-description').textContent = '';
  $('more-releases').hidden = true;
  $('install-release').disabled = true;
  $('versions-dialog').showModal();
  await loadReleases();
}
async function loadReleases() {
  const picker = releasePicker;
  if (!picker || picker.nextIndex === null) return;
  $('install-release').disabled = true;
  await run('Loading compatible releases…', async () => {
    const result = picker.url
      ? await window.inventory.urlReleases(picker.id, picker.url, picker.nextIndex)
      : await window.inventory.releases(picker.id, picker.folder, picker.nextIndex);
    if (releasePicker !== picker) return;
    if (picker.url)
      picker.latestUrl = result.url || picker.url.replace(/\/(?:files|download)\/\d+\/?$/, '');
    const previous = $('release-choice').value;
    picker.nextIndex = result.nextIndex;
    picker.installedFileId = result.installedFileId;
    picker.files.push(...result.files.filter((f) => !picker.files.some((p) => p.id === f.id)));
    $('versions-summary').textContent =
      result.name +
      ' · ' +
      result.flavor +
      (picker.url ? '' : ' · Installed: ' + (result.installedVersion || 'unknown'));
    $('release-choice').replaceChildren(
      ...(picker.url
        ? [
            Object.assign(document.createElement('option'), {
              value: 'latest',
              textContent: 'Latest compatible stable release (default)',
            }),
          ]
        : []),
      ...picker.files.map((f) => {
        const option = document.createElement('option');
        option.value = String(f.id);
        option.textContent =
          f.name +
          ' · ' +
          f.channel +
          ' · ' +
          new Date(f.date).toLocaleDateString() +
          (f.id === picker.installedFileId ? ' · Installed' : '');
        option.disabled = f.id === picker.installedFileId;
        return option;
      }),
    );
    $('release-choice').value = previous || (picker.url ? 'latest' : '');
    $('more-releases').hidden = picker.nextIndex === null;
    $('install-release').disabled = true;
    $('release-description').textContent = picker.files.length
      ? 'Choose the release you want to install.'
      : 'No compatible releases on this page.';
  });
  $('release-choice').onchange();
}
$('more-releases').onclick = loadReleases;
$('close-releases').onclick = () => $('versions-dialog').close();
$('versions-dialog').addEventListener('close', () => {
  releasePicker = null;
});
$('release-choice').onchange = () => {
  const f = releasePicker?.files.find((f) => f.id === Number($('release-choice').value));
  const latest = releasePicker?.url && $('release-choice').value === 'latest';
  $('install-release').disabled = !latest && (!f || f.id === releasePicker?.installedFileId);
  $('release-description').textContent = f
    ? 'Game versions: ' + f.gameVersions.join(', ') + ' · File #' + f.id
    : latest
      ? 'The latest compatible stable release will be installed.'
      : '';
};
$('install-release').onclick = async () => {
  const picker = releasePicker,
    file = picker?.files.find((f) => f.id === Number($('release-choice').value));
  const url = picker?.url && $('release-choice').value === 'latest' ? picker.latestUrl : file?.url;
  if (!url || (file && file.id === picker.installedFileId)) return;
  let plan;
  await run('Preparing selected release…', async () => {
    plan = await window.inventory.prepareUrl(picker.id, url);
  });
  if (!plan) return;
  $('versions-dialog').close();
  if (plan.blocked) {
    showDownloadFallback(plan);
    return;
  }
  acceptPlan(plan);
};
$('open-build-guide').onclick = () => window.inventory.openAboutLink('buildGuide').catch(error);
$('open-api-console').onclick = () => window.inventory.openAboutLink('apiConsole').catch(error);

let addonListSelection = null;
const listCheckboxes = () => [...$('list-entries').querySelectorAll('input[type="checkbox"]')];
function updateListSelection() {
  const count = listCheckboxes().filter((input) => input.checked).length;
  $('list-selected-count').textContent = count + ' of ' + listCheckboxes().length + ' selected';
  $('confirm-list').disabled = count === 0;
}
function openListSelection(mode, id, list) {
  addonListSelection = { mode, id, document: list };
  $('list-title').textContent = mode === 'export' ? 'Export addon list' : 'Import addon list';
  $('list-description').textContent =
    list.addons.length + ' addons for ' + list.flavor + '. Choose which addons to ' + mode + '.';
  $('list-help').textContent =
    mode === 'export'
      ? 'Share the JSON file with a friend using the same WoW client. Addons without a source will need manual installation.'
      : 'Selected addons will be downloaded into ' +
        selected().installation.name +
        '. Existing folders are backed up before replacement. A CurseForge API key is required. Known file versions are preserved; project links use the latest compatible release.';
  $('confirm-list').hidden = false;
  $('confirm-list').textContent = mode === 'export' ? 'Export selected' : 'Download and install';
  $('list-selection-actions').hidden = false;
  $('list-entries').replaceChildren(
    ...list.addons.map((addon, index) => {
      const label = document.createElement('label');
      label.className = 'list-addon';
      const input = document.createElement('input');
      input.type = 'checkbox';
      input.checked = true;
      input.value = String(index);
      const text = document.createElement('span');
      text.textContent =
        addon.name + ' · ' + addon.version + (addon.url ? '' : ' · Manual installation required');
      label.append(input, text);
      return label;
    }),
  );
  updateListSelection();
  // run() restores controls after the dialog opens.
  $('confirm-list').dataset.preBusyDisabled = String(list.addons.length === 0);
  $('list-dialog').showModal();
}
$('export-list').onclick = () =>
  run('Reading addon list…', async () => {
    const id = selected().id;
    openListSelection('export', id, await window.inventory.addonList(id));
  });
$('import-list').onclick = () =>
  run('Reading addon list…', async () => {
    const id = selected().id;
    const list = await window.inventory.chooseList(id);
    if (list) openListSelection('import', id, list);
  });
$('list-entries').onchange = updateListSelection;
for (const [id, checked] of [
  ['list-select-all', true],
  ['list-deselect-all', false],
]) {
  $(id).onclick = () => {
    listCheckboxes().forEach((input) => {
      input.checked = checked;
    });
    updateListSelection();
  };
}
$('cancel-list').onclick = () => $('list-dialog').close();
$('list-dialog').addEventListener('close', () => {
  addonListSelection = null;
});
$('confirm-list').onclick = () => {
  const pending = addonListSelection;
  const indices = listCheckboxes()
    .filter((input) => input.checked)
    .map((input) => Number(input.value));
  if (!pending || !indices.length) return;
  if (pending.mode === 'export') {
    run('Exporting addon list…', async () => {
      const result = await window.inventory.exportList(pending.id, indices);
      if (result) {
        $('list-dialog').close();
        notice('Exported ' + result.count + ' addons.');
      }
    });
    return;
  }
  const list = {
    ...pending.document,
    addons: indices.map((index) => pending.document.addons[index]),
  };
  $('list-dialog').close();
  run('Installing addon list…', async () => {
    const result = await window.inventory.installList(pending.id, list);
    state = result.state;
    render();
    if (result.failures.length) {
      $('list-title').textContent = 'Import results';
      $('list-description').textContent =
        'Installed ' + result.installed + '. Could not install ' + result.failures.length + ':';
      $('list-help').textContent =
        'The remaining addons were not installed. You can import them again or install them manually.';
      $('list-selection-actions').hidden = true;
      $('list-entries').textContent = result.failures
        .map((f) => f.name + ': ' + f.reason)
        .join('\n');
      $('confirm-list').hidden = true;
      $('list-dialog').showModal();
    } else notice('Installed ' + result.installed + ' addons.');
  });
};
$('library-menu').addEventListener('beforetoggle', (event) => {
  if (event.newState !== 'open') return;
  const rect = $('library-more').getBoundingClientRect();
  $('library-menu').style.left = Math.max(8, rect.right - 225) + 'px';
  $('library-menu').style.top = rect.bottom + 4 + 'px';
});
$('library-menu').addEventListener('click', (event) => {
  if (event.target.closest('button')) $('library-menu').hidePopover();
});

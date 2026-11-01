// Narrow native API. The page has no filesystem, shell or HTTP permissions.
const invoke = (method, ...args) =>
  window.__TAURI__.core.invoke('inventory', {
    method,
    args: args.map((a) => (a === undefined ? null : a)),
  });
const listen = (name, callback) => {
  let stopped = false;
  let remove;
  window.__TAURI__.event
    .listen(name, (e) => callback(e.payload))
    .then((fn) => {
      remove = fn;
      if (stopped) fn();
    })
    .catch((e) => console.error(e));
  return () => {
    stopped = true;
    remove?.();
  };
};
window.inventory = {
  onProgress: (callback) => listen('inventory:progress', callback),
  onDrop: (callback) => listen('inventory:drop', callback),
  addonList: (id) => invoke('addon-list', id),
  exportList: (id, indices) => invoke('export-list', id, indices),
  chooseList: (id) => invoke('choose-list', id),
  installList: (id, document) => invoke('install-list', id, document),
  desktopSettings: () => window.__TAURI__.core.invoke('desktop_settings'),
  setAutostart: (enabled) => window.__TAURI__.core.invoke('set_autostart', { enabled }),
  load: () => invoke('load'),
  checkAppUpdate: () => window.__TAURI__.core.invoke('check_app_update'),
  installAppUpdate: () => window.__TAURI__.core.invoke('install_app_update'),
  onAppUpdateProgress: (callback) => listen('app-update:progress', callback),
  about: () => invoke('about'),
  openAboutLink: (name) => invoke('open-about-link', name),
  keyStatus: () => invoke('key-status'),
  saveApiKey: (key) => invoke('save-api-key', key),
  clearApiKey: () => invoke('clear-api-key'),
  report: (id) => invoke('report', id),
  resolveSource: (id) => invoke('resolve-source', id),
  viewSource: (id, folder) => invoke('view-source', id, folder),
  showFolder: (id, folder) => invoke('show-folder', id, folder),
  chooseZip: () => invoke('choose-zip'),
  inspectZip: (id, file) => invoke('inspect-zip', id, file),
  installZip: (token) => invoke('install-zip', token),
  cancelImport: (token) => invoke('cancel-import', token),
  removeAddon: (id, folder) => invoke('remove-addon', id, folder),
  browse: (kind) => invoke('browse', kind),
  discover: (input) => invoke('discover', input),
  add: (roots) => invoke('add', roots),
  select: (id) => invoke('select', id),
  rescan: (id) => invoke('rescan', id),
  remove: (id) => invoke('remove', id),
  preferences: (data) => invoke('preferences', data),
  bind: (id, folder, url) => invoke('bind', id, folder, url),
  clientIcon: (id) => invoke('client-icon', id),
  releases: (id, folder, index = 0) => invoke('releases', id, folder, index),
  urlReleases: (id, url, index = 0) => invoke('url-releases', id, url, index),
  prepareUrl: (id, url) => invoke('prepare-url', id, url),
  checkUpdates: (id, folder) => invoke('check-updates', id, folder),
  openDownload: (url) => invoke('open-download', url),
  getIcon: (id, folder) => invoke('get-icon', id, folder),
  icon: (id, folder) => invoke('icon', id, folder),
};

(function (root) {
  // Known bundle layouts, never general dependency-based ownership.
  const bundles = [
    ['Bagnon', ['Bagnon', 'BagBrother', 'Bagnon_Bank', 'Bagnon_Config', 'Bagnon_GuildBank']],
    [
      'AtlasLootClassic',
      [
        'AtlasLootClassic',
        'AtlasBIStooltips',
        'AtlasLootClassic_Collections',
        'AtlasLootClassic_Crafting',
        'AtlasLootClassic_Data',
        'AtlasLootClassic_DungeonsAndRaids',
        'AtlasLootClassic_Factions',
        'AtlasLootClassic_Options',
        'AtlasLootClassic_PvP',
      ],
    ],
    [
      'DBM-Core',
      [
        'DBM-Core',
        'DBM-GUI',
        'DBM-StatusBarTimers',
        'DBM-Brawlers',
        'DBM-Lairs-Midnight',
        'DBM-Midnight',
        'DBM-Raids-Midnight',
        'DBM-VPVEM',
      ],
    ],
    [
      'Details',
      [
        'Details',
        'Details_Compare2',
        'Details_DataStorage',
        'Details_EncounterDetails',
        'Details_RaidCheck',
        'Details_Streamer',
        'Details_TinyThreat',
        'Details_Vanguard',
      ],
    ],
    ['TipTac', ['TipTac', 'TipTacItemRef', 'TipTacOptions', 'TipTacTalents']],
  ];
  function projectUrl(value) {
    try {
      const u = new URL(value);
      if (u.protocol !== 'https:' || u.hostname !== 'www.curseforge.com') return null;
      const parts = u.pathname.split('/').filter(Boolean);
      return parts.length === 3 && parts[0] === 'wow' && parts[1] === 'addons'
        ? 'https://www.curseforge.com/wow/addons/' + parts[2].toLowerCase()
        : null;
    } catch {
      return null;
    }
  }
  function groupAddons(addons, bindings = {}, packages = []) {
    const byFolder = new Map(addons.map((a) => [a.folder, a]));
    const parent = new Map(addons.map((a) => [a.folder, a.folder]));
    const find = (f) => {
      let p = parent.get(f);
      while (p !== parent.get(p)) p = parent.get(p);
      return p;
    };
    function compatible(a, b) {
      const aLink = bindings[a.folder]?.url,
        bLink = bindings[b.folder]?.url;
      if (aLink && bLink) return (projectUrl(aLink) || aLink) === (projectUrl(bLink) || bLink);
      if (a.curseProjectId && b.curseProjectId && a.curseProjectId !== b.curseProjectId)
        return false;
      return true;
    }
    function join(a, b) {
      const aMembers = addons.filter((x) => find(x.folder) === find(a.folder));
      const bMembers = addons.filter((x) => find(x.folder) === find(b.folder));
      if (aMembers.every((x) => bMembers.every((y) => compatible(x, y))))
        parent.set(find(b.folder), find(a.folder));
    }
    for (const [primary, folders] of bundles) {
      const main = byFolder.get(primary);
      if (!main) continue;
      for (const folder of folders) {
        const member = byFolder.get(folder);
        if (member) join(main, member);
      }
    }
    for (const record of packages) {
      const main =
        byFolder.get(record.primary) || record.folders.map((f) => byFolder.get(f)).find(Boolean);
      if (!main) continue;
      for (const folder of record.folders) {
        const member = byFolder.get(folder);
        if (member) join(main, member);
      }
    }
    const urls = new Map();
    for (const addon of addons) {
      const url = projectUrl(bindings[addon.folder]?.url);
      if (!url) continue;
      const previous = urls.get(url);
      if (previous) join(previous, addon);
      else urls.set(url, addon);
    }
    const projects = new Map();
    for (const addon of addons) {
      if (!addon.curseProjectId) continue;
      const previous = projects.get(addon.curseProjectId);
      if (previous) join(previous, addon);
      else projects.set(addon.curseProjectId, addon);
    }
    const groups = new Map();
    for (const addon of addons) {
      const key = find(addon.folder);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(addon);
    }
    return [...groups.values()].map((members) => {
      const manifest = packages.find((p) => members.some((a) => a.folder === p.primary));
      const known = bundles.find(([primary]) => members.some((a) => a.folder === primary));
      const primary =
        members.find((a) => a.folder === manifest?.primary) ||
        members.find((a) => a.folder === known?.[0]) ||
        [...members].sort(
          (a, b) =>
            Number(!!b.version) - Number(!!a.version) ||
            a.folder.length - b.folder.length ||
            a.folder.localeCompare(b.folder),
        )[0];
      const ordered = [
        primary,
        ...members.filter((a) => a !== primary).sort((a, b) => a.folder.localeCompare(b.folder)),
      ];
      return { ...primary, name: manifest?.title || primary.name, members: ordered };
    });
  }
  if (typeof module === 'object' && module.exports) module.exports = { groupAddons };
  else root.addonGroups = { groupAddons };
})(globalThis);

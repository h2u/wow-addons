const test=require('node:test');
const assert=require('node:assert/strict');
const {groupAddons}=require('../desktop/grouping.js');
const a=(folder,extra={})=>({folder,name:folder,version:'',dependencies:[],...extra});
test('Bagnon is one package including BagBrother, without changing inventory',()=>{
 const input=['BagBrother','Bagnon','Bagnon_Bank','Bagnon_Config','Bagnon_GuildBank'].map(f=>a(f,f==='Bagnon'?{version:'12.1.7'}:{}));
 const before=JSON.stringify(input),groups=groupAddons(input);
 assert.equal(groups.length,1);assert.equal(groups[0].folder,'Bagnon');assert.equal(groups[0].version,'12.1.7');assert.equal(groups[0].members.length,5);assert.equal(JSON.stringify(input),before);
});
test('dependencies and similar names do not merge independent addons',()=>{
 const groups=groupAddons([a('DBM-Core',{curseProjectId:3358}),a('DBM-GUI'),a('DBM-ThirdPartyVoice',{dependencies:['DBM-Core']}),a('BagnonExtra',{dependencies:['Bagnon']})]);
 assert.equal(groups.length,3);assert.equal(groups.find(g=>g.folder==='DBM-Core').members.length,2);
});
test('conflicting source IDs or manually linked projects remain separate',()=>{
 assert.equal(groupAddons([a('Bagnon',{curseProjectId:1}),a('Bagnon_Bank',{curseProjectId:2})]).length,2);
 assert.equal(groupAddons([a('Bagnon'),a('Bagnon_Bank')],{Bagnon:{url:'one'},Bagnon_Bank:{url:'two'}}).length,2);
});
test('shared project ID groups modules and missing primary does not swallow folders',()=>{
 const groups=groupAddons([a('TipTacItemRef',{curseProjectId:695153}),a('TipTacOptions',{curseProjectId:695153}),a('BagBrother')]);
 assert.equal(groups.length,2);assert.equal(groups.reduce((n,g)=>n+g.members.length,0),3);
});

test('AtlasLoot Forever includes BiS ToolTip from the same release archive',()=>{
 const folders=['AtlasBIStooltips','AtlasLootClassic','AtlasLootClassic_Collections','AtlasLootClassic_Crafting','AtlasLootClassic_Data','AtlasLootClassic_DungeonsAndRaids','AtlasLootClassic_Factions','AtlasLootClassic_Options','AtlasLootClassic_PvP'];
 const groups=groupAddons(folders.map(folder=>a(folder,folder==='AtlasLootClassic'?{version:'Forever 1.60.1',curseProjectId:1422985}:folder==='AtlasBIStooltips'?{version:'1.0.1',dependencies:['AtlasLootClassic']}:{})));
 assert.equal(groups.length,1);assert.equal(groups[0].folder,'AtlasLootClassic');assert.equal(groups[0].version,'Forever 1.60.1');assert.equal(groups[0].curseProjectId,1422985);assert.equal(groups[0].members.length,9);assert.ok(groups[0].members.some(m=>m.folder==='AtlasBIStooltips'));
});

test('DBM VEM voice pack is included in the DBM Core release',()=>{
 const groups=groupAddons([a('DBM-Core',{version:'12.1.18',curseProjectId:3358}),a('DBM-GUI'),a('DBM-VPVEM',{version:'a8e9f0e',dependencies:['DBM-Core']})]);
 assert.equal(groups.length,1);assert.equal(groups[0].folder,'DBM-Core');assert.equal(groups[0].version,'12.1.18');assert.equal(groups[0].members.length,3);
});
test('recorded package contents group modules without shared IDs or name prefixes',()=>{
 const input=[a('MyMain',{version:'1'}),a('UnrelatedModuleName')];
 const manifest=[{primary:'MyMain',folders:['MyMain','UnrelatedModuleName']}];
 assert.equal(groupAddons(input).length,2);assert.equal(groupAddons(input,{},manifest).length,1);assert.equal(groupAddons(input,{},manifest)[0].folder,'MyMain');
});

test('shared canonical CurseForge links group folders even without matching IDs',()=>{
 const bindings={First:{url:'https://www.curseforge.com/wow/addons/example/'},Second:{url:'https://www.curseforge.com/wow/addons/example?tab=files'}};
 const groups=groupAddons([a('First',{curseProjectId:1}),a('Second',{curseProjectId:2})],bindings);
 assert.equal(groups.length,1);assert.equal(groups[0].members.length,2);
});

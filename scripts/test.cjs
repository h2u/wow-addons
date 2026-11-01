const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),tests=fs.readdirSync(path.join(root,'tests')).filter(f=>f.endsWith('.test.cjs')).map(f=>path.join('tests',f));
if(!tests.length)throw new Error('No frontend tests found.');
const result=spawnSync(process.execPath,['--test',...tests],{cwd:root,stdio:'inherit'});if(result.error)throw result.error;process.exit(result.status??1);

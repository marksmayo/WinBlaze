import fs from 'node:fs';
import assert from 'node:assert/strict';
import {hasEqualFeatureSupport} from './medal-table.mjs';
const html=fs.readFileSync(new URL('./CURRENT_COMPARISON.html',import.meta.url),'utf8');
const visible=html.replace(/<script>[\s\S]*?<\/script>/g,'');
for(const id of ['evidence','repository-performance-audit','scan-export-review','extended','public-operations','coverage']) assert(!visible.includes('id="'+id+'"'),'Remove '+id+' from comparison');
assert(!visible.includes('Extended comparisons'),'Extended comparison section must be absent');
for(const label of ['Historical optimization evidence','Historical startup optimization evidence','100-point improvement audit']) assert(!visible.includes(label),'Remove '+label);
for(const [,id] of visible.matchAll(/href="#([^"]+)"/g)) assert(visible.includes('id="'+id+'"'),'Navigation target missing: '+id);
const data=JSON.parse(html.match(/const DATA = (.*);/)[1]);
const levels={none:0,partial:1,full:2};
const advantage=r=>Number.isFinite(levels[r.support.WinBlaze])&&['WinDirStat','WizTree'].every(t=>Number.isFinite(levels[r.support[t]]))&&levels[r.support.WinBlaze]>Math.max(levels[r.support.WinDirStat],levels[r.support.WizTree]);
const sharedGold=r=>r.support.WinBlaze==='full'&&['WinDirStat','WizTree'].some(t=>r.support[t]==='full');
for(const row of data.features.rows.filter(sharedGold)) {
 assert(!visible.includes('<b>'+row.feature+'</b>'),'Shared gold row must be omitted: '+row.feature);
 assert(!data.medals.events.some(e=>e.category==='feature'&&e.name===row.feature),'Shared gold must not count: '+row.feature);
}
for(const row of data.features.rows.filter(advantage)){
 assert(!visible.includes('<b>'+row.feature+'</b>'),'WinBlaze advantage row must be omitted: '+row.feature);
 assert(!data.medals.events.some(e=>e.category==='feature'&&e.name===row.feature),'WinBlaze advantage must not earn medals: '+row.feature);
}
const differing=data.features.rows.filter(r=>!hasEqualFeatureSupport(r.support)&&!advantage(r)&&!sharedGold(r));
assert.equal((visible.match(/data-parity-row=/g)||[]).length,0,'Feature parity section must be absent');
for(const row of data.features.rows.filter(r=>hasEqualFeatureSupport(r.support))) assert(!visible.includes('<b>'+row.feature+'</b>'),'Equal parity row still visible: '+row.feature);
assert(!visible.includes('id="feature-comparison"')&&!visible.includes('id="feature-table"'));
assert(visible.includes('Every medal, side by side')&&visible.includes('Know exactly what ran'));
console.log('PASS: focused report keeps current comparisons and versions, omits audit history and shared parity rows.');

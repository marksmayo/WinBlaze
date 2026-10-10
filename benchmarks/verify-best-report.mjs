import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {buildMedalTable,rankMedalEvents} from './medal-table.mjs';
import {retainCompetitorBests} from './competitor-bests.mjs';

const root=new URL('./',import.meta.url);
const template=fs.readFileSync(new URL('report-template.html.template',root),'utf8');
const d=JSON.parse(fs.readFileSync(new URL('../artifacts/all-gold-loop/baseline/CURRENT_COMPARISON.html',root),'utf8').match(/const DATA = (.*);/)[1]);
const identities=Object.fromEntries(['WinDirStat','WizTree'].map(tool=>[tool,d.versions.filter(r=>r.tool===tool).at(-1)]));
const old=retainCompetitorBests(null,buildMedalTable(d).events,identities,{source:'retained-fast-batch.json'});
for(const row of d.workflow) if(row.tool!=='WinBlaze') row.elapsed_ms*=2;
const raw=buildMedalTable(d);
const selected=retainCompetitorBests(old.store,raw.events,identities,{source:'slower-new-batch.json'});
d.latestMedals=raw;
d.medals={...raw,...rankMedalEvents(selected.events)};
const html=template.replace('/* BENCHMARK_DATA */',JSON.stringify(d).replace(/</g,'\\u003c'));
const elements=new Map([...html.matchAll(/id="([^"]+)"/g)].map(([,id])=>[id,{
    innerHTML:'',textContent:'',value:({profile:'full',resource:'memory','tool-filter':'all'})[id]||'',checked:false,
    addEventListener(){},insertAdjacentHTML(_position,value){this.innerHTML+=value;}
}]));
const context=vm.createContext({document:{getElementById(id){assert(elements.has(id),'Unknown report element '+id);return elements.get(id);}},Date,console,Blob,URL,setTimeout});
vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1],context);
const official=d.medals.events.find(e=>e.name==='Full C: volume · scan workflow');
for(const tool of ['WinDirStat','WizTree']) {
    const seconds=(official.times[tool]/1000).toFixed(2)+' s';
    assert(elements.get('cards').innerHTML.includes(seconds),'Official card must keep the faster retained result');
    assert(elements.get('scan-chart').innerHTML.includes(seconds),'Official scan chart must keep the faster retained result');
    assert(elements.get('workflow-chart').innerHTML.includes(seconds),'Official workflow chart must keep the faster retained result');
}
assert(elements.get('medal-events').innerHTML.includes('retained-fast-batch.json'),'Medal source must be visible without JavaScript');
assert(elements.get('workflow-stats').innerHTML.includes('Latest batch raw observations'),'Slower raw observations must remain labelled separately');
console.log('PASS: official cards/charts/medals retain faster competitor evidence; latest raw observations remain intact and explicitly labelled.');

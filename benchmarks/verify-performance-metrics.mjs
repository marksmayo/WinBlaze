import assert from 'node:assert/strict';
import fs from 'node:fs';
import {percentile, timingVariability, buildMedalTable, featureMedalCoverage} from './medal-table.mjs';

assert.equal(percentile([1,2,3,4,5,6,7,8,9,10],0.9),9.1);
assert.equal(percentile([10,1,NaN,null],0.5),5.5);
assert.equal(percentile([],0.9),null);
assert.equal(timingVariability([10,10,10,10]),0);
assert.equal(timingVariability([0,0,0]),null);
assert.equal(timingVariability([1,2,3,4]),60);
const html=fs.readFileSync(new URL('./CURRENT_COMPARISON.html',import.meta.url),'utf8');
const data=JSON.parse(html.match(/const DATA = (.*);/)[1]);
const result=buildMedalTable(data);
for(const label of ['Quick folder','Empty-file scale','Nonempty dense folder','Full C: volume']) {
    for(const metric of ['90th percentile completion','timing variability']) {
        const event=result.events.find(e=>e.name===`${label} · ${metric}`);
        assert(event,`${label} needs ${metric}`);
        assert(event.awards.length===3 && event.comparable);
        assert(Object.values(event.samples).every(n=>n>=10));
    }
}
for(const label of ['Quick folder','Empty-file scale','Nonempty dense folder']) {
    for(const metric of ['peak memory','private memory','CPU consumption']) {
        assert(result.events.some(e=>e.name===`${label} · ${metric}`));
    }
}
// Incomplete repetitions and old/rejected rows must not win tail/stability medals.
const sparse={...data,workflow:data.workflow.filter(r=>r.run<=2)};
assert(!buildMedalTable(sparse).events.some(e=>e.name.includes('90th percentile')||e.name.includes('timing variability')&&!e.name.startsWith('Startup')));
console.log('PASS: synthetic resources, p90 and normalized IQR medals; sample/identity guards and missing-value handling.');

const coverage=featureMedalCoverage(data,result.events);
assert.equal(coverage.filter(r=>r.status==='awarded').length,result.events.filter(event=>event.name.startsWith('Feature fixture · scan/CSV ')).length);
for(const row of coverage.filter(row=>row.status==='awarded')) assert(Object.values(row.counts).every(count=>count>=10),'Only current validated n=10 feature rows may earn medals');
for(const operation of new Set(coverage.map(r=>r.operation))) assert.equal(coverage.filter(r=>r.operation===operation).length,7);
assert(coverage.filter(r=>['output_bytes','load_ms','operation_ms'].includes(r.metric)).every(r=>r.status==='not-ranked'));
assert(coverage.find(r=>r.operation==='treemap-png'&&r.metric==='elapsed_ms').reason.includes('WinDirStat'));
console.log('PASS: every feature/metric combination has medals or an explicit eligibility reason.');

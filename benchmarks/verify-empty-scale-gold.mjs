import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {timingVariability,percentile} from './medal-table.mjs';

const path=process.argv[2]||new URL('./empty-scale-iteration10.json',import.meta.url);
const profile=process.argv[3]||'scale';
const workloads={scale:{files:16384,bytes:0},tiny:{files:72,bytes:73728},'fanout-large':{files:8192,bytes:2097152}};
const workload=workloads[profile];
assert(workload,'Known unchanged workload required');
const rows=JSON.parse(fs.readFileSync(path,'utf8').replace(/^\uFEFF/,'')).filter(row=>row.profile===profile);
assert.equal(rows.length,30,'Require all30 unfiltered n10 observations');
const scores={};
for(const tool of ['WinBlaze','WinDirStat','WizTree']) {
    const samples=rows.filter(row=>row.tool===tool);
    assert.equal(samples.length,10,'Require n10 for '+tool);
    assert.deepEqual(samples.map(row=>row.run).sort((a,b)=>a-b),[1,2,3,4,5,6,7,8,9,10]);
    const identity=samples[0].binary;
    const current=crypto.createHash('sha256').update(fs.readFileSync(identity.executable_path)).digest('hex').toUpperCase();
    for(const row of samples) {
        assert.equal(row.profile,profile);
        assert.equal(row.files,workload.files,'Complete fixture required');
        assert.equal(row.logical_bytes,workload.bytes);
        assert.equal(row.elevation,'standard user token');
        assert.equal(row.scope,'process startup + scan + CSV export (no GUI rendering)');
        assert.notEqual(row.included_in_summary,false,'No excluded observations');
        assert.equal(row.binary.sha256,current,'Binary changed since measurement');
        assert(Number.isFinite(row.elapsed_ms)&&row.elapsed_ms>0);
    }
    const values=samples.map(row=>row.elapsed_ms);
    scores[tool]=timingVariability(values);
    console.log(`${tool}: median ${percentile(values,.5).toFixed(2)} ms; variability ${scores[tool].toFixed(2)}%; n=10`);
}
assert(scores.WinBlaze<=Math.min(scores.WinDirStat,scores.WizTree),
    'WinBlaze has not earned gold for unchanged '+profile+' timing variability');
console.log('PASS: current-binary WinBlaze gold in the unchanged n10 '+profile+' variability event.');

import assert from 'node:assert/strict';
import {retainCompetitorBests} from './competitor-bests.mjs';

const identity = {product_version:'2.8.0',file_version:'2.8.0.2147',sha256:'AAA'};
const identities = {WinDirStat:identity,WizTree:{product_version:'4.33',file_version:'4.33.0.0',sha256:'BBB'}};
const event = (value, n=10, name='Full C: volume · scan workflow') => ({
    name,unit:'ms',scope:'startup + scan + CSV',comparable:true,observational:false,
    times:{WinBlaze:20000,WinDirStat:value,WizTree:30000},
    samples:{WinBlaze:10,WinDirStat:n,WizTree:10},
    measurements:{WinBlaze:Array(10).fill(20000),WinDirStat:Array(n).fill(value),WizTree:Array(10).fill(30000)}
});
let result=retainCompetitorBests(null,[event(13230)],identities,{source:'batch-a',recorded_utc:'2026-10-05T00:00:00Z'});
result=retainCompetitorBests(result.store,[event(15550)],identities,{source:'batch-b',recorded_utc:'2026-10-06T00:00:00Z'});
assert.equal(result.events[0].times.WinDirStat,13230,'A slower rerun must preserve the official best');
assert.equal(result.events[0].competitor_bests.WinDirStat.source,'batch-a');
assert.deepEqual(result.events[0].measurements.WinDirStat,Array(10).fill(13230));
assert.equal(result.events[0].times.WinBlaze,20000,'WinBlaze must use its current observations');
result=retainCompetitorBests(result.store,[event(12000)],identities,{source:'batch-c'});
assert.equal(result.events[0].times.WinDirStat,12000,'A validated improvement must replace the official best');
result=retainCompetitorBests(result.store,[event(1000,9)],identities,{source:'short-batch'});
assert.equal(result.events[0].times.WinDirStat,12000,'Nine samples cannot overwrite a ten-sample record');
const changed={...identities,WinDirStat:{...identity,product_version:'2.9.0',sha256:'CCC'}};
result=retainCompetitorBests(result.store,[event(17000)],changed,{source:'new-version'});
assert.equal(result.events[0].times.WinDirStat,17000,'A changed product version starts a separate baseline');
const changedHash={...changed,WinDirStat:{...changed.WinDirStat,sha256:'DDD'}};
result=retainCompetitorBests(result.store,[event(18000)],changedHash,{source:'new-binary'});
assert.equal(result.events[0].times.WinDirStat,18000,'A replaced binary cannot inherit another executable baseline');
const memory={...event(500,10,'Full C: private memory'),unit:'MiB'};
result=retainCompetitorBests(result.store,[memory],identities,{source:'memory-a'});
result=retainCompetitorBests(result.store,[{...memory,times:{...memory.times,WinDirStat:600},measurements:{...memory.measurements,WinDirStat:Array(10).fill(600)}}],identities,{source:'memory-b'});
assert.equal(result.events[0].times.WinDirStat,500,'Each resource metric retains its own minimum');
assert.equal(result.events[0].competitor_bests.WinDirStat.source,'memory-a');
assert.throws(()=>retainCompetitorBests({schema:999,entries:[]},[],identities,{}),/schema/i);
console.log('PASS: slower results cannot replace competitor records; all metrics retain validated batches, version/hash changes reset active baselines, WinBlaze remains current.');

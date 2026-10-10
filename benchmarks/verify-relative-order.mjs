import assert from 'node:assert/strict';
import fs from 'node:fs';
import {rankMedalEvents} from './medal-table.mjs';

const event = (name, wb, wd, wiz) => ({name, comparable:true,
    times:{WinBlaze:wb, WinDirStat:wd, WizTree:wiz}});
const input = [event('strongest advantage',25,100,200), event('3x gap',300,100,150),
    event('tie',100,100,200), event('4x gap',400,100,200), event('2x advantage',50,100,200)];
const result = rankMedalEvents(input);
assert.deepEqual(result.events.map(row=>row.name),
    ['4x gap','3x gap','tie','2x advantage','strongest advantage']);
assert.deepEqual(result.events.map(row=>row.relative_ratio),[4,3,1,0.5,0.25]);
assert.deepEqual(input.map(row=>row.name),
    ['strongest advantage','3x gap','tie','4x gap','2x advantage'], 'Do not mutate caller order');
assert.deepEqual(result.standings,rankMedalEvents([...input].reverse()).standings,
    'Changing display order must not change medals or podium rankings');
const zero = rankMedalEvents([event('zero WinBlaze',0,5,10),
    event('all zero',0,0,0),event('zero competitor',5,0,10),event('equal ratio',20,20,40)]);
assert.deepEqual(zero.events.map(row=>row.name),
    ['zero competitor','all zero','equal ratio','zero WinBlaze']);
assert.equal(zero.events[0].relative_ratio,null, 'Unbounded ratios remain valid JSON');
assert(zero.events[0].relative_label.includes('zero measured comparator value'));
assert.equal(zero.events[1].relative_ratio,1, 'All-zero ties have equal relative performance');
assert.equal(zero.events[3].relative_ratio,0);
const html = fs.readFileSync(new URL('./CURRENT_COMPARISON.html',import.meta.url),'utf8');
const data = JSON.parse(html.match(/const DATA = (.*);/)[1]);
const performance = data.medals.events.filter(row=>row.category!=='feature');
const ratio = row => {
    const best = Math.min(row.times.WinDirStat,row.times.WizTree);
    return best===0 ? (row.times.WinBlaze===0 ? 1 : Infinity) : row.times.WinBlaze/best;
};
for(let i=1;i<performance.length;i++) assert(ratio(performance[i-1])>=ratio(performance[i]),
    'Generated report must list weakest relative results first');
assert(html.includes('WinBlaze / best competitor'));
console.log('PASS: relative performance sorts largest gaps first and strongest advantages last.');

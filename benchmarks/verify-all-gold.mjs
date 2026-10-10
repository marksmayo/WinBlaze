import fs from 'node:fs';
import assert from 'node:assert/strict';
import { assertOriginalEvents } from './assert-original-events.mjs';

const report = process.argv[2] || new URL('./CURRENT_COMPARISON.html', import.meta.url);
const html = fs.readFileSync(report, 'utf8');
const embedded = html.match(/const DATA = (.*);/);
assert(embedded, 'Generated benchmark data must be embedded in the report');
const data = JSON.parse(embedded[1]);
assert.equal(data.batch.state, 'complete', 'Complete the benchmark batch before checking the goal');
assert(data.batch.requested_runs >= 10, 'The target requires n=10 reruns');
const events = data.medals.events;
assert(events.every(event=>!Object.values(event.pending_refresh||{}).some(Boolean)), 'Pending refreshes cannot complete the all-gold goal');
const failures = events.flatMap(event => {
    const award = event.awards.find(entry => entry.tool === 'WinBlaze');
    return award?.medal === 'gold' ? [] : [`${event.name}: ${award?.medal || 'not ranked'}`];
});
console.log(`WinBlaze gold: ${events.length - failures.length}/${events.length} eligible events`);
for (const failure of failures) console.log(failure);
assert.equal(events.length, 35, 'All 35 existing categories require current eligible evidence');
const original = JSON.parse(fs.readFileSync(new URL('./original-medal-events.json', import.meta.url), 'utf8'));
assertOriginalEvents(events, original.events);
for(const definition of original.events){const fresh=data.latestMedals.measurementEvents.find(event=>event.name===definition.name);for(const tool of ['WinBlaze','WinDirStat','WizTree'])assert.equal(fresh?.samples[tool],10,'Every original metric must be rerun with fresh n=10: '+definition.name+' '+tool);}

assert.equal(failures.length, 0, 'WinBlaze has not yet achieved gold in every category');
console.log('PASS: all 35 existing categories are gold in the completed n=10 report.');

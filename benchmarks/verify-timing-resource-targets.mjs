import fs from 'node:fs';
import assert from 'node:assert/strict';
import {timingVariability} from './medal-table.mjs';
const read = path => JSON.parse(fs.readFileSync(path, 'utf8').replace(/^\uFEFF/, ''));
const rows = read(process.argv[2]);
const html = fs.readFileSync(process.argv[3] || new URL('./CURRENT_COMPARISON.html', import.meta.url), 'utf8');
const report = JSON.parse(html.match(/const DATA = (.*);/)[1]);
const workloads = {tiny:['Quick folder',72,73728],scale:['Empty-file scale',16384,0],
    'fanout-large':['Nonempty dense folder',8192,2097152]};
const failures = [];
for (const [profile,[label,files,bytes]] of Object.entries(workloads)) {
    const samples = rows.filter(row => row.tool === 'WinBlaze' && row.profile === profile);
    assert.equal(samples.length, 10, 'Require ten complete CLI observations: ' + profile);
    assert.equal(new Set(samples.map(row => row.binary.sha256)).size, 1);
    assert.deepEqual(samples.map(row => row.run).sort((a,b)=>a-b), [1,2,3,4,5,6,7,8,9,10]);
    for (const row of samples) {
        assert.equal(row.files, files);
        assert.equal(row.logical_bytes, bytes);
        assert.equal(row.elevation, 'standard user token');
        assert.equal(row.scope, 'process startup + scan + CSV export (no GUI rendering)');
        assert.notEqual(row.included_in_summary, false);
        assert(Number.isFinite(row.elapsed_ms) && row.elapsed_ms > 0);
    }
    const observed = timingVariability(samples.map(row => row.elapsed_ms));
    const event = report.medals.events.find(event => event.name.startsWith(label)
        && event.name.endsWith('timing variability'));
    const target = Math.min(event.times.WinDirStat,event.times.WizTree);
    console.log(`${profile}: variability ${observed.toFixed(4)}%; retained competitor target ${target.toFixed(4)}%`);
    if(observed > target)failures.push(profile);
}
assert.equal(failures.length,0,'Remaining complete scan/export variability deficits: '+failures.join(', '));
console.log('PASS: all three synthetic CLI variability targets meet retained competitor records.');

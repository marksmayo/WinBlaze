import fs from 'node:fs';
import assert from 'node:assert/strict';
const read = filename => JSON.parse(fs.readFileSync(filename, 'utf8').replace(/^\uFEFF/, ''));
const rows = read(process.argv[2]);
const report = fs.readFileSync(process.argv[3] || new URL('./CURRENT_COMPARISON.html', import.meta.url), 'utf8');
const data = JSON.parse(report.match(/const DATA = (.*);/)[1]);
const accepted = rows.filter(row => row.tool === 'WinBlaze' && row.idle_reached);
assert.equal(accepted.length, 10, 'Require the unchanged ten-run first-idle protocol');
assert.equal(new Set(accepted.map(row => row.binary.identity_sha256)).size, 1, 'One binary identity per batch');
const failures = [];
for (const [name, key, divisor] of [
    ['Startup CPU at first idle', 'cpu_ms_at_idle', 1000],
    ['Startup working set at first idle', 'working_set_bytes_at_idle', 1048576],
    ['Startup peak working set at first idle', 'peak_working_set_bytes_at_idle', 1048576]
]) {
    const values = accepted.map(row => row[key] / divisor).sort((a, b) => a - b);
    assert(values.every(value => Number.isFinite(value) && value > 0), 'Complete positive resource observations');
    const observed = (values[4] + values[5]) / 2;
    const event = data.medals.events.find(event => event.name === name);
    const target = Math.min(event.times.WinDirStat, event.times.WizTree);
    console.log(`${name}: ${observed.toFixed(6)} ${event.unit}; retained competitor target ${target.toFixed(6)}`);
    if (observed > target) failures.push(name);
}
assert.equal(failures.length, 0, 'Remaining startup resource deficits: ' + failures.join(', '));
console.log('PASS: unchanged first-idle resource metrics meet retained competitor records.');

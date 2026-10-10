import fs from 'node:fs';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {percentile} from './medal-table.mjs';

const input=new URL('./startup-peak-delayed-shell.json',import.meta.url);
const rows=JSON.parse(fs.readFileSync(input,'utf8').replace(/^\uFEFF/,''));
const temporary=new URL('../artifacts/startup-peak-invalid-report.json',import.meta.url);
for(const mutate of [
    data=>data.pop(),
    data=>data[0].included_in_summary=false,
    data=>data[0].idle_reached=false,
    data=>data[0].run=2,
    data=>data[0].peak_working_set_bytes_at_idle=1,
    data=>data[0].binary.sha256='changed',
]){
    const invalid=structuredClone(rows);
    mutate(invalid);
    fs.writeFileSync(temporary,JSON.stringify(invalid));
    const result=spawnSync(process.execPath,['benchmarks/render-startup-peak-report.mjs',fileURLToPath(temporary)],{encoding:'utf8'});
    assert.notEqual(result.status,0,'Reject incomplete, excluded, invalid or changed-binary evidence');
    assert.match(result.stderr,/AssertionError/,'Reject evidence through validation, not missing-file errors');
}
fs.unlinkSync(temporary);
const rendered=spawnSync(process.execPath,['benchmarks/render-startup-peak-report.mjs',fileURLToPath(input)],{encoding:'utf8'});
assert.equal(rendered.status,0,rendered.stderr);
const html=fs.readFileSync(new URL('./STARTUP_PEAK_WORKING_SET.html',import.meta.url),'utf8');
assert(!html.includes('<select'),'All medal measurements are visible without dropdowns');
assert(html.includes('width=device-width'),'Responsive report required');
for(const tool of ['WinBlaze','WinDirStat','WizTree']){
    const samples=rows.filter(row=>row.tool===tool);
    assert(html.includes(percentile(samples.map(row=>row.peak_working_set_bytes_at_idle/1048576),.5).toFixed(2)+' MiB'));
    assert(html.includes(samples[0].binary.sha256));
}
for(const name of ['Startup peak working set at first idle','Startup working set at first idle','Startup CPU at first idle','Startup · first idle message queue']) assert(html.includes(name));
console.log('PASS: responsive report shows all startup medals, current binaries and exact n10 medians; invalid evidence is rejected');

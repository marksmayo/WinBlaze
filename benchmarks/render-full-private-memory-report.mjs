import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {percentile} from './medal-table.mjs';

const input=process.argv[2]||new URL('./full-private-memory-current.json',import.meta.url);
const rows=JSON.parse(fs.readFileSync(input,'utf8').replace(/^\uFEFF/,'')).filter(row=>row.profile==='full');
const products=['WinBlaze','WinDirStat','WizTree'];
assert.equal(rows.length,30,'Require complete n=10 per product');
const summary=products.map(tool=>{
    const samples=rows.filter(row=>row.tool===tool);
    assert.equal(samples.length,10);
    assert.deepEqual(samples.map(row=>row.run).sort((a,b)=>a-b),[1,2,3,4,5,6,7,8,9,10]);
    const binary=samples[0].binary;
    const hash=crypto.createHash('sha256').update(fs.readFileSync(binary.executable_path)).digest('hex').toUpperCase();
    for(const row of samples){
        assert.equal(row.scope,'process startup + scan + CSV export (no GUI rendering)');
        assert.equal(row.elevation,'standard user token');
        assert.notEqual(row.included_in_summary,false);
        assert.notEqual(row.baseline_current,false);
        assert.equal(row.binary.sha256,hash,'Measured executable changed');
        assert(Number.isFinite(row.peak_private_bytes_sampled)&&row.peak_private_bytes_sampled>0);
        assert(row.sample_count>0&&row.files>0&&row.elapsed_ms>0);
    }
    return {tool,binary,score:percentile(samples.map(row=>row.peak_private_bytes_sampled/1048576),.5),
        files:percentile(samples.map(row=>row.files),.5),time:percentile(samples.map(row=>row.elapsed_ms),.5)};
}).sort((a,b)=>a.score-b.score);
const medal=row=>['🥇 Gold','🥈 Silver','🥉 Bronze'][summary.findIndex(other=>other.score===row.score)];
const escape=value=>String(value).replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const maximum=Math.max(...summary.map(row=>row.score));
const html=`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>WinBlaze · Full-drive private memory</title><style>
body{margin:0;background:#f3f5f8;color:#142033;font:16px system-ui}main{max-width:1150px;margin:auto;padding:5vw}
h1{font-size:clamp(2rem,5vw,4rem);letter-spacing:-.05em;margin:0}p{color:#526278;line-height:1.6}
.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}.card,section{background:white;border:1px solid #e0e6ef;border-radius:18px;padding:24px;margin-top:24px}
.value{font-size:2.4rem;font-weight:750}.bar{height:12px;background:#eef1f5;border-radius:9px;overflow:hidden;margin:15px 0}.bar span{display:block;height:100%}
.table{overflow:auto}table{border-collapse:collapse;width:100%;white-space:nowrap}th,td{text-align:left;padding:12px;border-bottom:1px solid #e5eaf1}
code{font-size:11px;overflow-wrap:anywhere}small{color:#526278}@media(max-width:700px){.grid{grid-template-columns:1fr}.card{margin-top:10px}}
</style><main><small>STANDARD-USER · FULL-DRIVE PRIVATE MEMORY</small><h1>A full drive.<br>A measured footprint.</h1>
<p>Live C: volume · ten runs per product · process startup + complete scan/indexing/persistence + CSV export. Median sampled peak private memory; lower wins. All slow and high-memory runs remain included.</p>
<div class="grid">${summary.map(row=>`<article class="card"><b>${medal(row)} · ${row.tool}</b><div class="value">${row.score.toFixed(2)} MiB</div><div class="bar"><span style="width:${row.score/maximum*100}%;background:${{WinBlaze:'#ef4537',WinDirStat:'#467ade',WizTree:'#199f91'}[row.tool]}"></span></div><small>n=10 · median ${(row.time/1000).toFixed(2)} s<br>${Math.round(row.files).toLocaleString('en-US')} files at median</small></article>`).join('')}</div>
<section><h2>Measurement and medals</h2><div class="table"><table><thead><tr><th>Event</th>${products.map(tool=>`<th>${tool}</th>`).join('')}</tr></thead><tbody><tr><td>Standard-user · full-drive private memory</td>${products.map(tool=>{const row=summary.find(item=>item.tool===tool);return `<td>${row.score.toFixed(2)} MiB · ${medal(row)}</td>`;}).join('')}</tr></tbody></table></div>
<p>WinBlaze reuses advisory allocation counts only for the same requested root and scan rules; every run still completes a fresh scan, indexing, persistence and export. The first cold run is included. Private bytes represent committed private process memory, not resident working set. Sampling every 25 ms can miss short peaks. Caches and background activity are uncontrolled. This live volume changes between scans, and accessible-file totals can differ by product; the medal describes this measured workflow rather than equal per-file efficiency.</p></section>
<section><h2>Every private-memory observation</h2><div class="table"><table><thead><tr><th>Run</th>${products.map(tool=>`<th>${tool} · MiB</th>`).join('')}</tr></thead><tbody>${Array.from({length:10},(_,index)=>`<tr><td>${index+1}</td>${products.map(tool=>`<td>${(rows.find(row=>row.tool===tool&&row.run===index+1).peak_private_bytes_sampled/1048576).toFixed(2)}</td>`).join('')}</tr>`).join('')}</tbody></table></div></section>
<section><h2>Measured binaries</h2>${summary.map(row=>`<p><b>${row.tool}</b> · ${escape(row.binary.product_version||row.binary.file_version||'local build')}<br><code>${escape(row.binary.sha256)}</code></p>`).join('')}<small>Focused comparison; no claim that all 35 original events have been refreshed.</small></section></main></html>`;
fs.writeFileSync(new URL('./FULL_DRIVE_PRIVATE_MEMORY.html',import.meta.url),html);
for(const row of summary)console.log(`${row.tool}: ${row.score.toFixed(2)} MiB · ${medal(row)} · n=10`);
if(process.argv.includes('--require-gold')) assert.equal(summary.find(row=>row.tool==='WinBlaze').score,summary[0].score,'WinBlaze has not earned gold');

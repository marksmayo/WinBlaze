import fs from 'node:fs';
import assert from 'node:assert/strict';
import {percentile,timingVariability} from './medal-table.mjs';

const input=process.argv[2]||new URL('./empty-scale-iteration10.json',import.meta.url);
const profile=process.argv[3]||'scale';
const workloads={
    scale:{files:16384,bytes:0,title:'Empty-file',headline:'Empty files.',description:'16,384 empty files',output:'EMPTY_SCALE_VARIABILITY.html'},
    tiny:{files:72,bytes:73728,title:'Quick-folder',headline:'Quick folders.',description:'72 files across 13 directories',output:'QUICK_FOLDER_VARIABILITY.html'},
    'fanout-large':{files:8192,bytes:2097152,title:'Dense-folder',headline:'Dense folders.',description:'8,192 nonempty 256-byte files across 3 directories',output:'DENSE_FOLDER_VARIABILITY.html'},
};
const workload=workloads[profile];
assert(workload,'Known unchanged workload required');
const rows=JSON.parse(fs.readFileSync(input,'utf8').replace(/^\uFEFF/,'')).filter(row=>row.profile===profile);
const products=['WinBlaze','WinDirStat','WizTree'];
assert.equal(rows.length,30,'Render a complete n10 batch only');
const summary=products.map(tool=>{
    const samples=rows.filter(row=>row.tool===tool);
    assert.equal(samples.length,10);
    assert.deepEqual(samples.map(row=>row.run).sort((a,b)=>a-b),[1,2,3,4,5,6,7,8,9,10]);
    for(const row of samples){
        assert.equal(row.profile,profile);
        assert.equal(row.files,workload.files);
        assert.equal(row.logical_bytes,workload.bytes);
        assert.equal(row.elevation,'standard user token');
        assert.equal(row.scope,'process startup + scan + CSV export (no GUI rendering)');
        assert.notEqual(row.included_in_summary,false,'Retain all ten observations');
        assert.equal(row.binary.sha256,samples[0].binary.sha256);
        assert(Number.isFinite(row.elapsed_ms)&&row.elapsed_ms>0);
    }
    const times=samples.map(row=>row.elapsed_ms);
    return {tool,median:percentile(times,.5),score:timingVariability(times),binary:samples[0].binary};
}).sort((a,b)=>a.score-b.score);
const maximum=Math.max(1,...summary.map(row=>row.score));
const escape=value=>String(value).replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const result=summary.findIndex(row=>row.tool==='WinBlaze');
const medal=['gold','silver','bronze'][summary.findIndex(row=>row.score===summary[result].score)];
const html=`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>WinBlaze · ${workload.title} consistency</title><style>
body{margin:0;background:#f3f5f8;color:#142033;font:16px system-ui}main{max-width:1100px;margin:auto;padding:5vw}
h1{font-size:clamp(2rem,5vw,4rem);letter-spacing:-.05em;margin:0}p{color:#526278;line-height:1.6}
.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}.card,section{background:white;border:1px solid #e0e6ef;border-radius:18px;padding:24px;margin-top:24px}
.value{font-size:2.4rem;font-weight:750}.bar{height:12px;background:#eef1f5;border-radius:9px;overflow:hidden;margin:15px 0}.bar span{display:block;height:100%;background:#ef4537}
.table{overflow:auto}table{border-collapse:collapse;width:100%;white-space:nowrap}th,td{text-align:left;padding:12px;border-bottom:1px solid #e5eaf1}
code{font-size:11px;overflow-wrap:anywhere}small{color:#526278}@media(max-width:700px){.grid{grid-template-columns:1fr}.card{margin-top:10px}}
</style><main><small>FOCUSED PERFORMANCE COMPARISON</small><h1>${workload.headline}<br>Consistent completion.</h1>
<p>${workload.description} · ten runs per product · startup + complete scan/indexing + CSV export.<br>Timing variability = interquartile range ÷ median × 100. Lower is more consistent.</p>
<div class="grid">${summary.map((row,index)=>`<article class="card"><b>${['🥇','🥈','🥉'][summary.findIndex(other=>other.score===row.score)]} ${row.tool}</b><div class="value">${row.score.toFixed(2)}%</div><div class="bar"><span style="width:${row.score/maximum*100}%"></span></div><small>Median ${row.median.toFixed(2)} ms · n=10</small></article>`).join('')}</div>
<section><h2>Every measured completion</h2><div class="table"><table><thead><tr><th>Run</th>${products.map(tool=>`<th>${tool} · ms</th>`).join('')}</tr></thead><tbody>
${Array.from({length:10},(_,index)=>`<tr><td>${index+1}</td>${products.map(tool=>`<td>${rows.find(row=>row.tool===tool&&row.run===index+1).elapsed_ms.toFixed(2)}</td>`).join('')}</tr>`).join('')}
</tbody></table></div><p>This measured build earns ${medal} for WinBlaze. No slow runs were removed. Caches and machine activity are uncontrolled; this batch does not prove a causal saving from an individual code change.</p>${profile==='fanout-large'?'<p>All 8,192 paths and logical sizes match the fixture. Allocated totals differ for resident files: WinBlaze/WinDirStat report 256 bytes per file; WizTree reports zero. Windows metadata and extent queries independently verified this difference. <a href="https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_get_retrieval_pointers">Resident-data extent semantics</a>.</p>':''}</section>
<section><h2>Measured binaries</h2>${summary.map(row=>`<p><b>${row.tool}</b> · ${escape(row.binary.product_version||row.binary.file_version||'local build')}<br><code>${escape(row.binary.sha256)}</code></p>`).join('')}
<small>This is a focused measured snapshot, not a claim that the complete 35-category benchmark batch passed.</small></section></main></html>`;
fs.writeFileSync(new URL('./'+workload.output,import.meta.url),html);
console.log(`Rendered n10 ${profile} comparison: WinBlaze ${medal} (${summary[result].score.toFixed(2)}%).`);

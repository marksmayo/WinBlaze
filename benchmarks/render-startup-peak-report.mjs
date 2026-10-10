import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {percentile} from './medal-table.mjs';

const input=process.argv[2]||new URL('./startup-peak-delayed-shell.json',import.meta.url);
const rows=JSON.parse(fs.readFileSync(input,'utf8').replace(/^\uFEFF/,''));
const products=['WinBlaze','WinDirStat','WizTree'];
const definitions=[
    ['Startup peak working set at first idle','peak_working_set_bytes_at_idle','MiB',1048576],
    ['Startup working set at first idle','working_set_bytes_at_idle','MiB',1048576],
    ['Startup CPU at first idle','cpu_ms_at_idle','CPU-s',1000],
    ['Startup · first idle message queue','initialization_ms','ms',1],
];
const hash=path=>crypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex').toUpperCase();
assert.equal(rows.length,30,'Require all thirty n10 observations');
for(const tool of products){
    const samples=rows.filter(row=>row.tool===tool);
    assert.equal(samples.length,10);
    assert.deepEqual(samples.map(row=>row.run).sort((a,b)=>a-b),[1,2,3,4,5,6,7,8,9,10]);
    for(const row of samples){
        assert.equal(row.idle_reached,true);
        assert.notEqual(row.included_in_summary,false);
        assert.notEqual(row.baseline_current,false);
        assert.equal(row.scope,'launch request to first idle GUI message queue; hidden window; not usable-window time');
        assert.equal(row.binary.sha256,hash(row.executable_path),'Executable changed since measurement');
        if(row.binary.native_dll_sha256){
            const dll=row.executable_path.replace(/[^\\/]+$/,'winblaze_native.dll');
            assert.equal(row.binary.native_dll_sha256,hash(dll),'Native companion changed since measurement');
        }
        for(const [,key] of definitions) assert(Number.isFinite(row[key])&&row[key]>0,key);
        assert(row.peak_working_set_bytes_at_idle>=row.working_set_bytes_at_idle);
    }
}
const metrics=definitions.map(([name,key,unit,divisor])=>{
    const values=Object.fromEntries(products.map(tool=>[tool,percentile(rows.filter(row=>row.tool===tool).map(row=>row[key]/divisor),.5)]));
    const sorted=products.map(tool=>values[tool]).sort((a,b)=>a-b);
    return {name,key,unit,divisor,values,medals:Object.fromEntries(products.map(tool=>[tool,['🥇','🥈','🥉'][sorted.indexOf(values[tool])]])),relative:values.WinBlaze/Math.min(values.WinDirStat,values.WizTree)};
}).sort((a,b)=>b.relative-a.relative);
const target=metrics.find(row=>row.key==='peak_working_set_bytes_at_idle');
const escape=value=>String(value).replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const maximum=Math.max(...Object.values(target.values));
const colors={WinBlaze:'#ef4537',WinDirStat:'#447ee8',WizTree:'#159d8c'};
const html=`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>WinBlaze · Startup memory comparison</title><style>
body{margin:0;background:#f3f5f8;color:#142033;font:16px system-ui}main{max-width:1150px;margin:auto;padding:5vw}h1{font-size:clamp(2rem,5vw,4rem);letter-spacing:-.05em;margin:0}p{color:#526278;line-height:1.6}.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}.card,section{background:white;border:1px solid #e0e6ef;border-radius:18px;padding:24px;margin-top:24px}.value{font-size:2.4rem;font-weight:750}.bar{height:12px;background:#eef1f5;border-radius:9px;overflow:hidden;margin:15px 0}.bar span{display:block;height:100%}.table{overflow:auto}table{border-collapse:collapse;width:100%;white-space:nowrap}th,td{text-align:left;padding:12px;border-bottom:1px solid #e5eaf1}code{font-size:11px;overflow-wrap:anywhere}small{color:#526278}@media(max-width:700px){.grid{grid-template-columns:1fr}.card{margin-top:10px}}
</style><main><small>FOCUSED PERFORMANCE COMPARISON · N=10</small><h1>Startup.<br>Memory at first idle.</h1>
<p>Peak working set observed when the hidden process first reports an idle GUI message queue. Lower uses less resident memory at this boundary. It includes the process lifetime peak up to the counter read; it does not measure a visible, usable window or completed cache restoration.</p>
<div class="grid">${products.map(tool=>`<article class="card"><b>${target.medals[tool]} ${tool}</b><div class="value">${target.values[tool].toFixed(2)} MiB</div><div class="bar"><span style="background:${colors[tool]};width:${target.values[tool]/maximum*100}%"></span></div><small>Median peak working set · n=10</small></article>`).join('')}</div>
<section><h2>Every startup measurement</h2><p>Sorted from WinBlaze’s largest relative deficit to its strongest result. Each cell shows the median and its medal; lower is better.</p><div class="table"><table><thead><tr><th>Measurement</th>${products.map(tool=>`<th>${tool}</th>`).join('')}<th>WinBlaze / best competitor</th></tr></thead><tbody>${metrics.map(metric=>`<tr><td>${metric.name}</td>${products.map(tool=>`<td>${metric.medals[tool]} ${metric.values[tool].toFixed(metric.unit==='CPU-s'?3:2)} ${metric.unit}</td>`).join('')}<td>${metric.relative.toFixed(2)}×</td></tr>`).join('')}</tbody></table></div></section>
<section><h2>Every measured peak</h2><div class="table"><table><thead><tr><th>Run</th>${products.map(tool=>`<th>${tool} · MiB</th>`).join('')}</tr></thead><tbody>${Array.from({length:10},(_,index)=>`<tr><td>${index+1}</td>${products.map(tool=>`<td>${(rows.find(row=>row.tool===tool&&row.run===index+1).peak_working_set_bytes_at_idle/1048576).toFixed(2)}</td>`).join('')}</tr>`).join('')}</tbody></table></div><p>All ten observations are retained. Separate batches and uncontrolled machine activity do not establish savings from individual edits.</p></section>
<section><h2>Measured versions</h2>${products.map(tool=>{const row=rows.find(row=>row.tool===tool);return `<p><b>${tool}</b> · ${escape(row.binary.product_version||row.binary.file_version||'local build')} · ${escape(row.utc)}<br><code>${escape(row.binary.sha256)}</code>${row.binary.native_dll_sha256?`<br><small>Native companion</small> <code>${escape(row.binary.native_dll_sha256)}</code>`:''}</p>`;}).join('')}<small>Focused startup results; other benchmark categories have not been refreshed for this UI build.</small></section></main></html>`;
fs.writeFileSync(new URL('./STARTUP_PEAK_WORKING_SET.html',import.meta.url),html);
console.log(`Rendered current-binary n10 startup comparison: WinBlaze ${target.medals.WinBlaze} ${target.values.WinBlaze.toFixed(2)} MiB.`);
if(process.argv.includes('--require-gold')) assert(target.values.WinBlaze<=Math.min(target.values.WinDirStat,target.values.WizTree),'WinBlaze has not earned startup peak working set gold');

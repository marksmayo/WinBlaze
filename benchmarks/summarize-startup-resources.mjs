import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
const root=path.resolve('benchmarks');
const read=p=>JSON.parse(fs.readFileSync(p,'utf8').replace(/^\uFEFF/,''));
const baseline=fs.readFileSync('artifacts/startup-resource-baseline-path.txt','utf8').replace(/^\uFEFF/,'').trim();
const median=a=>{const s=a.filter(Number.isFinite).sort((a,b)=>a-b);return (s[4]+s[5])/2;};
const results=[];
for(const [file,scope,fields] of [
 ['current-startup-results.json','First idle GUI message queue',{initialization_ms:'ms',cpu_ms_at_idle:'ms',working_set_bytes_at_idle:'bytes',peak_working_set_bytes_at_idle:'bytes'}],
 ['current-ui-ready-results.json','Fresh visible maximized window and enabled scan controls',{ready_ms:'ms',cpu_ms:'ms',working_set_bytes:'bytes'}]
]){
 const before=read(path.join(baseline,file)).filter(r=>r.tool==='WinBlaze');
 const after=read(path.join(root,file)).filter(r=>r.tool==='WinBlaze');
 assert.equal(before.length,10);assert.equal(after.length,10);
 for(const row of after){
  assert.equal(crypto.createHash('sha256').update(fs.readFileSync(row.binary.executable_path)).digest('hex').toUpperCase(),row.binary.sha256);
  assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(path.dirname(row.binary.executable_path),'winblaze_native.dll'))).digest('hex').toUpperCase(),row.binary.native_dll_sha256);
 }
 for(const [metric,unit] of Object.entries(fields)){
  assert(after.every(r=>Number.isFinite(r[metric]))&&before.every(r=>Number.isFinite(r[metric])));
  const old=median(before.map(r=>r[metric])),fresh=median(after.map(r=>r[metric]));
  results.push({scope,metric,unit,before:old,after:fresh,n:10,percent_lower:(old-fresh)/old*100});
 }
}
const data={generated_utc:new Date().toISOString(),runs:10,source_targets:50,shared_production_changes:5,baseline_directory:baseline,method:'Identical protocols, separate dated batches, caches and scheduling uncontrolled. CPU is coarse process telemetry. No per-target causal attribution.',results};
fs.writeFileSync(path.join(root,'current-startup-resource-validation.json'),JSON.stringify(data,null,2)+'\n');
for(const row of results)console.log(row.metric+': '+row.before+' → '+row.after+' '+row.unit+' ('+row.percent_lower.toFixed(1)+'% lower observed)');

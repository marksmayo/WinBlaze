import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url));
const baseline=fs.readFileSync(path.join(root,'../artifacts/repository-performance-baseline-path.txt'),'utf8').replace(/^\uFEFF/,'').trim();
const read=(dir,name)=>JSON.parse(fs.readFileSync(path.join(dir,name),'utf8').replace(/^\uFEFF/,''));
const median=values=>{const a=values.filter(Number.isFinite).sort((a,b)=>a-b);return a.length?(a[Math.floor((a.length-1)/2)]+a[Math.floor(a.length/2)])/2:null;};
const results=[];
const newMeasurements=[];
const specs=[
 ['current-workflow-results.json',r=>r.tool==='WinBlaze',r=>r.profile,{elapsed_ms:['Process time','ms',1],cpu_ms_sampled_lower_bound:['CPU','CPU-s',1000],peak_working_set_bytes_sampled:['Peak working set','MiB',1048576],peak_private_bytes_sampled:['Peak private memory','MiB',1048576]}],
 ['current-engine-results.json',r=>r.tool.startsWith('WinBlaze'),r=>r.profile,{scan_ms:['Engine scan','ms',1]}],
 ['current-gui-results.json',()=>true,r=>r.profile,{scan_ms:['GUI scan','ms',1],sampled_peak_working_set_bytes_through_observation:['GUI peak working set','MiB',1048576]}],
 ['current-feature-performance-results.json',r=>r.tool==='WinBlaze'&&r.status==='measured'&&r.operation!=='raw-mft-dump',r=>r.operation,{elapsed_ms:['Feature process time','ms',1],cpu_ms_sampled_lower_bound:['Feature CPU','CPU-s',1000],peak_working_set_bytes_sampled:['Feature peak working set','MiB',1048576]}],
 ['current-startup-results.json',r=>r.tool==='WinBlaze',()=> 'hidden-first-idle',{initialization_ms:['Startup idle proxy','ms',1]}],
 ['current-ui-ready-results.json',r=>r.tool==='WinBlaze',()=> 'fresh-visible-ready',{ready_ms:['Visible UI ready','ms',1]}],
 ['current-cache-results.json',()=>true,()=> 'full-snapshot-reopen',{native_decode_ms:['Cache decode','ms',1]}],
 ['current-incremental-results.json',()=>true,r=>r.mutation,{scan_ms:['Incremental scan','ms',1]}],
 ['current-index-memory-results.json',()=>true,r=>String(r.files),{'metrics.working_set_bytes':['Index working set','MiB',1048576],'metrics.elapsed_ms':['Index operation time','ms',1]}],
 ['current-public-operation-results.json',()=>true,r=>String(r.files),{'metrics.tree_ms':['Tree build','ms',1],'metrics.csv_buffer_ms':['CSV buffer export','ms',1],'metrics.search_name_top10_ms':['Name top-10 search','ms',1],'metrics.search_path_top10_ms':['Path top-10 search','ms',1],'metrics.search_size_top10_ms':['Size top-10 search','ms',1]}]
];
const get=(row,key)=>key.split('.').reduce((value,part)=>value?.[part],row);
let validated=0;
for(const [file,filter,group,metrics] of specs){
 const before=read(baseline,file).filter(filter),after=read(root,file).filter(filter);
 validated+=after.length;
 for(const row of after){
  assert(row.binary?.executable_path&&row.binary.sha256,file+' missing binary identity');
  const hash=crypto.createHash('sha256').update(fs.readFileSync(row.binary.executable_path)).digest('hex').toUpperCase();
  assert.equal(hash,row.binary.sha256.toUpperCase(),file+' stale executable');
  if(row.binary.native_dll_sha256){const dll=path.join(path.dirname(row.binary.executable_path),'winblaze_native.dll');assert.equal(crypto.createHash('sha256').update(fs.readFileSync(dll)).digest('hex').toUpperCase(),row.binary.native_dll_sha256.toUpperCase(),file+' stale native DLL');}
 }
 for(const name of new Set(after.map(group))){
  const a=after.filter(r=>group(r)===name),b=before.filter(r=>group(r)===name);
  assert.equal(a.length,10,file+' '+name+' must have n=10');
  if(b.length===0){newMeasurements.push({source:file,workload:name,n:a.length,scope:a[0].scope,reason:'New workload; no historical baseline'});continue;}
  assert.equal(b.length,10,file+' '+name+' baseline must have n=10');
  for(const [key,[label,unit,divisor]] of Object.entries(metrics)){
   const av=a.map(r=>get(r,key)).filter(Number.isFinite),bv=b.map(r=>get(r,key)).filter(Number.isFinite);
   if(av.length!==10||bv.length!==10)continue;
   const current=median(av)/divisor,previous=median(bv)/divisor;
   results.push({source:file,workload:name,metric:label,unit,before:previous,after:current,before_n:10,after_n:10,percent_lower:previous>0?(previous-current)/previous*100:null,scope:a[0].scope||'Same operation boundary; separate dated batches, caches uncontrolled'});
  }
 }
}
const manifest=read(root,'current-feature-performance-batch.json');
const featureJobs=manifest.jobs.filter(r=>r.tool==='WinBlaze'&&r.kind!=='mft');
const expected=290+featureJobs.length*10;
assert.equal(validated,expected,'Complete refresh must validate all current observations');
const full=results.filter(r=>r.source==='current-workflow-results.json'&&r.workload==='full');
const timing=full.find(r=>r.unit==='ms'),cpu=full.find(r=>r.unit==='CPU-s'),memory=full.find(r=>r.metric==='Peak working set');
const summary=`n=10 full-drive startup + scan + CSV export: ${(timing.before/1000).toFixed(2)} → ${(timing.after/1000).toFixed(2)} s (${formatObservedChange(timing.percent_lower, 'time')}). CPU ${cpu.before.toFixed(2)} → ${cpu.after.toFixed(2)} CPU-s; sampled peak working set ${memory.before.toFixed(2)} → ${memory.after.toFixed(2)} MiB. All ${validated} fresh WinBlaze observations match current executable/DLL hashes. Separate dated batches and uncontrolled caches prevent per-edit causal attribution. Comparator dates remain recorded independently in their observations.`;
fs.writeFileSync(path.join(root,'current-repository-performance-validation.json'),JSON.stringify({generated_utc:new Date().toISOString(),runs:10,validated_observations:validated,baseline_directory:baseline,summary,results,new_measurements:newMeasurements},null,2)+'\n');
console.log(summary);
import { formatObservedChange } from './format-observed-change.mjs';

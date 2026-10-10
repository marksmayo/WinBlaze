import fs from 'node:fs';
import path from 'node:path';
const root=path.resolve('benchmarks');
const beforeRoot=fs.readFileSync('artifacts/optimization-baseline-path.txt','utf8').replace(/^\uFEFF/,'').trim();
const read=(dir,name)=>JSON.parse(fs.readFileSync(path.join(dir,name),'utf8').replace(/^\uFEFF/,''));
const versions=read(root,'tool-version-history.json');
const current=r=>{if(!r.binary)return true;const latest=versions.filter(v=>v.tool===r.binary.tool).at(-1);return !latest||(latest.identity_sha256||latest.sha256)===(r.binary.identity_sha256||r.binary.sha256);};
const median=xs=>{xs.sort((a,b)=>a-b);const n=xs.length;return n%2?xs[n>>1]:(xs[n/2-1]+xs[n/2])/2};
const rows=[];
function add(scope,metric,unit,before,after,key,scale=1){
 const values=xs=>xs.filter(r=>r.included_in_summary!==false&&r.status!=='failed'&&Number.isFinite(r[key])&&r[key]>0).map(r=>r[key]/scale);
 const b=values(before),a=values(after.filter(current));if(!a.length||!b.length)return;
 const old=median(b),fresh=median(a);
 rows.push({scope,metric,unit,before:old,after:fresh,before_n:b.length,after_n:a.length,change_percent:100*(fresh-old)/old});
}
const before=read(beforeRoot,'current-workflow-results.json'),after=read(root,'current-workflow-results.json');
for(const profile of ['tiny','scale','fanout-large','full']){
 const b=before.filter(r=>r.tool==='WinBlaze'&&r.profile===profile),a=after.filter(r=>r.tool==='WinBlaze'&&r.profile===profile);
 for(const [name,key,unit,scale] of [['Elapsed','elapsed_ms','ms',1],['Sampled CPU lower bound','cpu_ms_sampled_lower_bound','CPU-s',1000],['Sampled peak working set','peak_working_set_bytes_sampled','MiB',1048576],['Sampled peak private bytes','peak_private_bytes_sampled','MiB',1048576]])add(`${profile} scan/index/persist/CSV process`,name,unit,b,a,key,scale);
}
add('GUI startup (frontend also changed since baseline)','First idle proxy','ms',read(beforeRoot,'current-startup-results.json').filter(r=>r.tool==='WinBlaze'),read(root,'current-startup-results.json').filter(r=>r.tool==='WinBlaze'),'initialization_ms');
const bEngine=read(beforeRoot,'current-batch-engine-results.json'),aEngine=read(root,'current-engine-results.json');
for(const profile of ['tiny','scale','fanout-large','full']){
 const b=bEngine.filter(r=>r.profile===profile),a=aEngine.filter(r=>r.profile===profile);
 for(const [name,key,unit,scale] of [['Scan time','scan_ms','ms',1],['Sampled CPU lower bound','cpu_ms_sampled_lower_bound','CPU-s',1000],['Sampled peak working set','peak_working_set_bytes_sampled','MiB',1048576]])add(`${profile} engine only (allocation queries added since baseline)`,name,unit,b,a,key,scale);
}
const bGui=read(beforeRoot,'current-gui-results.json'),aGui=read(root,'current-gui-results.json');
for(const profile of ['tiny','scale','full']){
 add(`${profile} GUI app-reported scan (frontend also changed)`,'Scan time','ms',bGui.filter(r=>(r.profile||r.scenario?.split('-')[0])===profile),aGui.filter(r=>r.profile===profile),'scan_ms');
}
add('GUI cache decode (different catalog sizes; single baseline observation)','Decode time','ms',read(beforeRoot,'current-cache-results.json'),read(root,'current-cache-results.json'),'native_decode_ms');
const bNative=read(beforeRoot,'current-elevated-native-results.json'),aNative=read(root,'current-elevated-native-results.json');
for(const [name,key,unit,scale] of [['Scan + index publication to Completed','scan_ms','ms',1],['Whole process including deferred write/tree queries','process_ms','ms',1],['Sampled CPU lower bound','cpu_ms_sampled_lower_bound','CPU-s',1000],['Sampled peak working set','peak_working_set_bytes_sampled','MiB',1048576]])add('Administrator native full pipeline',name,unit,bNative,aNative,key,scale);
const bFeatures=read(beforeRoot,'current-feature-performance-results.json'),aFeatures=read(root,'current-feature-performance-results.json');
for(const job of ['wb-scan-csv','wb-cache-load','wb-cached-csv','wb-extensions']){
 const b=bFeatures.filter(r=>r.job===job&&r.status==='measured'),a=aFeatures.filter(r=>r.job===job&&r.status==='measured');
 add(`224-file feature fixture / ${job}`,'Elapsed','ms',b,a,'elapsed_ms');
}
const data={baseline_path:beforeRoot,generated_utc:new Date().toISOString(),note:'Sequential before/after observations on a live disk with uncontrolled caches; percentages are descriptive, not causal estimates. Negative means lower after. The current build also includes parity features and per-file allocation queries, so changes are not isolated optimization savings. GUI startup includes frontend changes since the old baseline. Unsampled short-process resources are missing, not zero. Nine tested production optimizations are implemented; 91 other audit items are not new production optimizations.',rows};
fs.writeFileSync(path.join(root,'current-optimization-results.json'),JSON.stringify(data,null,2)+'\n');
console.log(rows.filter(r=>r.scope.startsWith('full')||r.scope.startsWith('GUI')));

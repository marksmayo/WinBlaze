import assert from 'node:assert/strict';
import {buildMedalTable} from './medal-table.mjs';

const products=['WinBlaze','WinDirStat','WizTree'];
const data={competitors:[],elevated:[],featurePerformance:[],features:{rows:[]},gui:[],native:[],startup:[],batch:{requested_runs:10},workflow:[]};
for(const [index,tool] of products.entries()) {
    for(let run=1;run<=10;run++) data.workflow.push({tool,run,profile:'full',elapsed_ms:100*(index+1),
        peak_private_bytes_sampled:1048576*(index+1),peak_working_set_bytes_sampled:1048576*(index+1),cpu_ms_sampled_lower_bound:10*(index+1)});
}
const name='Standard-user · full-drive private memory';
assert.equal(buildMedalTable(data).events.find(event=>event.name===name).awards.length,3);
data.workflow.find(row=>row.tool==='WinBlaze'&&row.run===10).peak_private_bytes_sampled=null;
assert(!buildMedalTable(data).events.some(event=>event.name===name),
    'Nine valid memory samples must not earn a medal in an n=10 comparison');
assert(buildMedalTable(data).events.some(event=>event.name==='Full C: volume · scan workflow'),
    'Missing memory telemetry must not discard complete timing observations');
console.log('PASS: medals require n=10 valid samples per product; missing telemetry never drops retained timing runs.');

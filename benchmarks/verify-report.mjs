import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {rankMedalEvents, isParityFeature} from './medal-table.mjs';
const root = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(root, 'CURRENT_COMPARISON.html'), 'utf8');
// A file preview may disable scripts. Results must already exist in the markup.
const staticHtml = html.replace(/<script>[\s\S]*?<\/script>/g, '');
for (const id of ['podium', 'medal-events', 'cards', 'scan-chart', 'startup-chart', 'resource-chart', 'versions', 'version-history', 'elevated', 'workflow-chart','distribution-chart','feature-performance-chart','feature-performance-results','feature-performance-coverage']) {
    assert(!staticHtml.includes(`id="${id}"></div>`) && !staticHtml.includes(`id="${id}"></tbody>`), `${id} must contain results without JavaScript`);
}
assert(staticHtml.includes('Startup') && staticHtml.includes('CPU-s'), 'Measured workflow and resource sections must be visible without JavaScript');
assert(/class="bar/.test(staticHtml), 'Measured bars must be visible without JavaScript');
assert(staticHtml.includes('Administrator-token comparison'), 'Elevated coverage must remain visible');
assert(!/<select\b/i.test(staticHtml),'Comparison results must not require dropdown selection');
const embedded = JSON.parse(html.match(/const DATA = (.*);/)[1]);
const medalComparison=staticHtml.match(/<table class="medal-comparison"[\s\S]*?<\/table>/)?.[0];
assert(medalComparison,'Every medal event must be visible in a product comparison table');
assert.equal((medalComparison.match(/data-medal-event=/g)||[]).length,embedded.medals.events.length);
for(const tool of ['WinBlaze','WinDirStat','WizTree']) assert(medalComparison.includes('<th scope="col">'+tool+'</th>'));
for(const [index,event] of embedded.medals.events.entries()) {
    const row=medalComparison.match(new RegExp('<tr data-medal-event="'+index+'">([\\s\\S]*?)</tr>'))?.[1];
    assert(row,event.name+' must have its own comparison row');
    for(const award of event.awards) assert(row.includes('data-tool="'+award.tool+'" data-medal="'+award.medal+'"'),event.name+' must show '+award.tool+' medal');
    for(const tool of ['WinBlaze','WinDirStat','WizTree']) {
        const cell=row.match(new RegExp('<td data-tool="'+tool+'"[^>]*>([\\s\\S]*?)</td>'))?.[1];
        assert(cell,event.name+' must show '+tool+' value');
        if(event.category==='feature') {
            const label={full:'Full · 3/3',partial:'Partial · 2/3',none:'None · 1/3',unverified:'Not verified'}[event.support[tool]]||'Not verified';
            assert(cell.includes(label),event.name+' must show its recorded support score');
        } else {
            const value=event.times[tool];
            const label=event.unit==='ms'?(value>=1000?(value/1000).toFixed(2)+' s':value>0&&value<0.001?'&lt;0.001 ms':(value>0&&value<1?value.toFixed(3):Math.round(value))+' ms'):value.toFixed(2)+' '+event.unit;
            assert(cell.includes(label),event.name+' must show its measured value and units');
            assert(cell.includes('n='+event.samples[tool]),event.name+' must show accepted sample count');
        }
    }
}
const headline=staticHtml.match(/id="cards">([\s\S]*?)<div class="notice"/)[1];
assert.equal((headline.match(/class="card"/g)||[]).length,3,'Headline must compare exactly three products using the common workflow');
assert(!headline.includes('WinBlaze GUI')&&!headline.includes('WinBlaze engine'));
assert.equal((headline.match(/Startup \+ scan \+ CSV export/g)||[]).length,3);
for (const tool of ['WinBlaze','WinDirStat','WizTree']) {
    const values=embedded.workflow.filter(r=>r.tool===tool&&r.profile==='full'&&r.baseline_current!==false&&r.included_in_summary!==false).map(r=>r.elapsed_ms).sort((a,b)=>a-b);
    assert.equal(values.length,10);
    const official=embedded.medals.events.find(e=>e.name==='Full C: volume · scan workflow');
    const seconds=(official.times[tool]/1000).toFixed(2)+' s';
    assert(headline.includes(seconds),`${tool} headline must use its official whole-process median`);
    if(tool!=='WinBlaze') {
        const best=official.competitor_bests[tool];
        assert.equal(best.samples,10);
        assert.equal(best.values.length,10);
        assert.equal(best.value,official.times[tool]);
        assert(staticHtml.includes(best.source),'Retained competitor source must remain visible');
    }
}


assert.equal(embedded.uiReady.length,10);
assert.equal(embedded.uiReady.filter(r=>r.baseline_current!==false&&r.included_in_summary!==false).length,10);
assert(staticHtml.includes('Current release · visible-ready startup'));
assert(staticHtml.includes('Validated n=10.'));
const readyValues=embedded.uiReady.map(r=>r.ready_ms).sort((a,b)=>a-b);
const readyMedian=(readyValues[4]+readyValues[5])/2;
const readyCell=staticHtml.match(/<h3>Current release · visible-ready startup<\/h3>[\s\S]*?<tbody><tr><td>([\d.]+) (ms|s)<\/td>/);
assert(readyCell,'The current visible-ready median must appear in its result cell');
const readyDisplayedMs=Number(readyCell[1])*(readyCell[2]==='s'?1000:1);
assert(Math.abs(readyDisplayedMs-readyMedian)<=(readyCell[2]==='s'?5:0.5)+1e-9,
    'Visible-ready display must equal the measured median at its displayed precision');
assert(staticHtml.includes('visible-ready startup'));
for (const row of embedded.medals.standings) {
    assert(staticHtml.includes(`${row.tool}, rank ${row.place}, ${row.gold} gold, ${row.silver} silver, ${row.bronze} bronze`));
}
assert(embedded.medals.events.some(e=>e.unit==='MiB'));
assert.equal(embedded.medals.events.filter(e=>e.name.includes('90th percentile completion')||e.name.includes('timing variability')).length,8);
assert(staticHtml.includes('Slow runs &amp; consistency')||staticHtml.includes('Slow runs & consistency'));
assert(embedded.medals.events.some(e=>e.unit==='CPU-s'));
assert(embedded.features.rows.length >= 10);
assert(embedded.features.rows.find(r=>r.feature==='Direct NTFS / MFT scan').WizTree.toLowerCase().includes('administrator mode required'));

const equalSupport=row=>['full','partial','none'].includes(row.support.WinBlaze)&&['WinDirStat','WizTree'].every(tool=>row.support[tool]===row.support.WinBlaze);
assert.equal(embedded.medals.events.filter(e=>e.category==='feature').length,embedded.features.rows.filter(row=>isParityFeature(row.support)).length);
for (const [level,badge] of [['full','🥇 Full'],['partial','🥈 Partial'],['none','🥉 None']]) {
    if (embedded.parityFeatures.some(r=>Object.values(r.support).includes(level))) assert(staticHtml.includes(badge));
}
assert(!embedded.medals.events.some(e=>e.category==='feature'&&e.name==='CSV export'),'Shared CSV support must not add medals');
assert(embedded.features.rows.some(row=>row.feature==='CSV export'));

for (const row of embedded.features.rows) {
    assert(row.criterion && row.verification, 'Every feature needs an audited scoring scope');
    const event=embedded.medals.events.find(item=>item.category==='feature'&&item.name===row.feature);
    if(!isParityFeature(row.support)){assert(!event,row.feature+' must not award shared-parity medals');continue;}
    assert(event,row.feature+' differing subfeature must be a medal event');
    for(const tool of ['WinBlaze','WinDirStat','WizTree']) {
        const expected={full:'gold',partial:'silver',none:'bronze'}[row.support[tool]];
        assert.equal(event.awards.find(award=>award.tool===tool)?.medal,expected,row.feature+' '+tool);
    }
}
assert.equal((staticHtml.match(/data-parity-row=/g)||[]).length,0);
assert(!staticHtml.includes('id="evidence"')&&!staticHtml.includes('Historical optimization evidence'));
for (const job of ['wb-json','wb-filter','wb-import','wb-import-wd','wb-import-wiz','wb-advanced-filter','wb-largest','wb-png','wb-duplicates','wb-permissions','wb-actions','wb-watch']) {
    const rows=embedded.featurePerformance.filter(r=>r.job===job&&r.status==='measured'&&r.baseline_current!==false);
    assert.equal(rows.length,10,`${job} must have ten validated current observations`);
    assert(rows.every(r=>r.validation?.verified));
}

assert(embedded.featurePerformance.some(r=>r.job==='wb-mft'),'Raw WinBlaze dump must have measured or explicit pending coverage');
assert(staticHtml.includes('CPU-s') && staticHtml.includes('measurement'));
const medals = rankMedalEvents([
    {comparable:true,times:{WinBlaze:10,WinDirStat:20,WizTree:30}},
    {comparable:true,times:{WinBlaze:10,WinDirStat:10,WizTree:30}},
    {comparable:false,times:{WinBlaze:1,WinDirStat:2,WizTree:3}},
    {comparable:true,times:{WinBlaze:1,WinDirStat:2}}
]);
assert.equal(medals.events.length,2, 'Mixed scopes and incomplete events cannot award medals');
assert.deepEqual(medals.standings.map(r=>[r.tool,r.gold,r.silver,r.bronze]),[
    ['WinBlaze',2,0,0],['WinDirStat',1,1,0],['WizTree',0,0,2]
]);
assert.deepEqual(medals.events.find(e=>e.times.WinDirStat===10).awards.map(a=>a.place),[1,1,3], 'Exact timing ties share medals');
const observed = rankMedalEvents([{observational:true,unit:'MiB',times:{WinBlaze:500,WinDirStat:250,WizTree:750}}]);
assert.equal(observed.standings[0].tool,'WinDirStat','Lower resource consumption receives gold');
const featureMedals = rankMedalEvents([
    {category:'feature',name:'Shared feature',support:{WinBlaze:'full',WinDirStat:'full',WizTree:'full'}},
    {category:'feature',name:'Support levels',support:{WinBlaze:'partial',WinDirStat:'full',WizTree:'none'}},
    {category:'feature',name:'Unverified',support:{WinBlaze:'none',WinDirStat:'unknown',WizTree:'partial'}}
]);
assert.equal(featureMedals.events.length,2);
assert.deepEqual(featureMedals.events[0].awards.map(a=>a.medal),['silver','gold','bronze']);
assert.equal(featureMedals.events[1].awards.length,2,'Unverified support receives no medal');
assert.deepEqual(featureMedals.standings.map(r=>[r.tool,r.gold,r.silver,r.bronze]),[
    ['WinDirStat',1,0,0],['WinBlaze',0,1,1],['WizTree',0,1,1]
]);
for(const level of ['full','partial','none']) {
    const shared=rankMedalEvents([{category:'feature',name:'Equal '+level,support:{WinBlaze:level,WinDirStat:level,WizTree:level}}]);
    assert.equal(shared.events.length,0);
    assert(shared.standings.every(row=>row.gold===0&&row.silver===0&&row.bronze===0));
}
for(const support of [
    {WinBlaze:'full',WinDirStat:'none',WizTree:'none'},
    {WinBlaze:'full',WinDirStat:'partial',WizTree:'none'},
    {WinBlaze:'partial',WinDirStat:'none',WizTree:'none'}
]) {
    const result=rankMedalEvents([{category:'feature',name:'WinBlaze advantage',support}]);
    assert.equal(result.events.length,0);
    assert(result.standings.every(row=>row.gold===0&&row.silver===0&&row.bronze===0));
}
for (const support of [
    {WinBlaze:'full',WinDirStat:'full',WizTree:'none'},
    {WinBlaze:'full',WinDirStat:'partial',WizTree:'full'},
    {WinBlaze:'full',WinDirStat:'unknown',WizTree:'full'}
]) {
    assert(!isParityFeature(support), 'WinBlaze shared gold must be omitted');
    const result = rankMedalEvents([{category:'feature',name:'Shared gold',support}]);
    assert.equal(result.events.length, 0);
    assert(result.standings.every(row=>row.gold===0&&row.silver===0&&row.bronze===0));
}
assert(isParityFeature({WinBlaze:'partial',WinDirStat:'full',WizTree:'partial'}), 'Remaining parity gaps stay visible');
assert(isParityFeature({WinBlaze:'full',WinDirStat:'unknown',WizTree:'none'}),'Unknown support requires verification, not assumed absence');
const templateLanding = fs.readFileSync(path.join(root, 'report-template.html'), 'utf8');
assert(templateLanding.includes('href="CURRENT_COMPARISON.html"'), 'Old template URL must link to the finished report');
assert(!templateLanding.includes('/* BENCHMARK_DATA */'), 'Old template URL must not expose the invalid data placeholder');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const elements = new Map();
const defaults = {profile:'full',resource:'memory','tool-filter':'all'};
for (const [,id] of html.matchAll(/id="([^"]+)"/g)) {
    elements.set(id, {innerHTML:'',textContent:'',value:defaults[id]||'',checked:false,
        addEventListener(){},insertAdjacentHTML(_position,value){this.innerHTML+=value;}});
}
const context = vm.createContext({document:{getElementById:id=>{
    assert(elements.has(id), `Unknown element ${id}`); return elements.get(id);
}},console,Date,Blob,URL,setTimeout});
vm.runInContext(script,context);
assert(elements.get('cards').innerHTML.length > 0);
for (const tool of ['WinDirStat','WizTree']) {
    const identity=embedded.versions.filter(row=>row.tool===tool).at(-1);
    assert(identity?.product_version, 'Current competitor version must be recorded');
    assert(elements.get('versions').innerHTML.includes(identity.product_version),
        'Version table must display the recorded current '+tool+' version');
}
for (const profile of ['full','tiny','scale','fanout-large']) {
    elements.get('profile').value=profile;
    vm.runInContext('scans()',context);
    assert(!elements.get('scan-chart').innerHTML.includes('NaN'));
    assert(elements.get('scan-chart').innerHTML.includes('WinDirStat'));
}
elements.get('warm').checked=true;
vm.runInContext('scans()',context);
elements.get('resource').value='cpu';
vm.runInContext('resources()',context);
assert(elements.get('resource-chart').innerHTML.includes('CPU-s'));
elements.get('resource').value='private';vm.runInContext('resources()',context);
assert(elements.get('resource-chart').innerHTML.includes('MiB'));
assert(!elements.get('resource-chart').innerHTML.includes('NaN'));
assert(!html.includes('/* BENCHMARK_DATA */'));
assert(!/<(?:script|link)[^>]+(?:src|href)="https?:/.test(html), 'Report must work offline');
console.log('Static results with scripts removed, template landing page, report scripts, dataset filters, CPU selector and exclusion handling verified.');

for (const operation of ['content-duplicates','json-export','permission-export','treemap-png','raw-mft-dump','scan-csv-fixture','file-actions','change-monitoring','advanced-filtered-export','largest-files','windirstat-csv-import','wiztree-csv-import']) {
    elements.get('feature-operation').value=operation;
    for (const metric of ['elapsed_ms','cpu_ms_sampled_lower_bound','peak_working_set_bytes_sampled','peak_private_bytes_sampled','output_bytes','load_ms','operation_ms']) {
        elements.get('feature-metric').value=metric;
        vm.runInContext('featurePerformance()',context);
        assert(!elements.get('feature-performance-chart').innerHTML.includes('NaN'));
    }
}
assert(elements.get('feature-performance-coverage').innerHTML.includes('Raw MFT dump export'));
assert(elements.get('feature-performance-coverage').innerHTML.includes('Duplicate detection'));
for(const profile of ['tiny','scale','fanout-large','full']) {
 elements.get('profile').value=profile;
 for(const metric of ['90th percentile completion','timing variability']) {
  elements.get('distribution-metric').value=metric;
  vm.runInContext('distributions();resources()',context);
  assert(elements.get('distribution-chart').innerHTML.includes('accepted n=10'));
  assert(!elements.get('distribution-chart').innerHTML.includes('NaN'));
 }
}
console.log('Feature workload selectors, resources, output validation status and per-feature coverage verified.');

assert(staticHtml.includes('Whole-process elevated resource use'));

assert(embedded.medals.events.some(e=>e.name==='Startup CPU at first idle'));
assert(embedded.medals.events.some(e=>e.name==='Startup working set at first idle'));

assert(embedded.medals.events.some(e=>e.name==='Standard-user \u00b7 full-drive private memory'));
assert(embedded.medals.events.some(e=>e.name==='Startup peak working set at first idle'));

assert(staticHtml.includes('WinBlaze \u00b7 whole native process'));
assert(staticHtml.includes('deferred snapshot write and tree queries'));

if (!embedded.native.some(r=>r.included_in_summary!==false&&r.baseline_current!==false)) {
    assert(staticHtml.includes('Historical administrator measurements'));
    assert(staticHtml.includes('excluded from current medians and medals'));
    assert(embedded.native.filter(r=>r.historical_eligible).every(r=>r.baseline_current===false&&r.included_in_summary===false));
}

assert.equal(vm.runInContext('podiumHeight(1)',context),240);
assert.equal(vm.runInContext('podiumHeight(2)',context),180);
assert.equal(vm.runInContext('podiumHeight(3)',context),125);
for(const row of embedded.medals.standings){
    assert(staticHtml.includes(`--height:${[240,180,125][row.place-1]}px`));
}

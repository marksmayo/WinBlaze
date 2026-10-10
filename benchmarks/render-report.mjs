import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import crypto from 'node:crypto';
import {buildMedalTable, featureMedalCoverage, isParityFeature, rankMedalEvents} from './medal-table.mjs';
import {retainCompetitorBests} from './competitor-bests.mjs';
import {retainLiveMedals, medalEvidence} from './live-medal-results.mjs';

const root = path.dirname(fileURLToPath(import.meta.url));
const read = name => fs.existsSync(path.join(root, name)) ? JSON.parse(fs.readFileSync(path.join(root, name), 'utf8').replace(/^\uFEFF/, '')) : [];
const data = {
    generated: new Date().toISOString(),
    competitors: [...read('current-competitor-results.json'), ...read('current-extended-competitor-results.json')],
    engine: read('current-engine-results.json'),
    gui: read('current-gui-results.json'),
    startup: read('current-startup-results.json'),
    cache: read('current-cache-results.json'),
    versions: read('tool-version-history.json'),
    inventory: read('current-binary-inventory.json'),
    built: read('current-built-inventory.json'),
    elevated: read('current-elevated-competitor-results.json'),
    mft: read('current-elevated-mft-results.json'),
    native: read('current-elevated-native-results.json'),
    elevatedStatus: read('current-elevated-status.json'),
    incremental: read('current-incremental-results.json'),
    competitorGui: read('current-competitor-gui-results.json'),
    allocation: read('current-allocation-results.json'),
    responsiveness: read('current-responsiveness-results.json'),
    coverage: read('current-extended-coverage.json'),
    environment: read('current-environment.json')
};
data.uiReady = read('current-ui-ready-results.json');
data.features = read('feature-comparison.json');
data.parityFeatures = data.features.rows.filter(row => isParityFeature(row.support));
data.indexMemory = read('current-index-memory-results.json');
data.publicOperations = read('current-public-operation-results.json');
data.workflow = read('current-workflow-results.json');
data.batch = read('current-benchmark-batch.json');
const standardFeatures=read('current-feature-performance-results.json');
const standaloneMft=read('current-feature-mft-results.json');
const standardMft=standardFeatures.filter(r=>r.operation==='raw-mft-dump');
const newest=rows=>Math.max(0,...rows.map(r=>Date.parse(r.started_utc)||0));
// Choose the newest batch per job; preserve a new product's pending entry
// even when an older standalone batch contains only the other product.
const mftBatch=[...new Set([...standardMft,...standaloneMft].map(r=>r.job))].flatMap(job=>{
    const standard=standardMft.filter(r=>r.job===job);
    const standalone=standaloneMft.filter(r=>r.job===job);
    return newest(standalone)>newest(standard)?standalone:standard.length?standard:standalone;
});
data.featurePerformance=[...standardFeatures.filter(r=>r.operation!=='raw-mft-dump'),...mftBatch,...read('current-portable-device-results.json'),...read('current-mft-reopen-results.json')];
// Raw desktop text can contain personal directory names. Embed numeric evidence only.
data.gui = data.gui.map(({status, ...row}) => row);
const latest = tool => data.versions.filter(r => r.tool === tool).at(-1);
const liveHashes = new Map();
const liveHash = filename => {
    if (!liveHashes.has(filename)) {
        try {
            liveHashes.set(filename, crypto.createHash('sha256')
                .update(fs.readFileSync(filename)).digest('hex').toUpperCase());
        } catch {
            liveHashes.set(filename, null);
        }
    }
    return liveHashes.get(filename);
};
const matchesLiveBinary = binary => {
    if (!binary.executable_path) return true; // Legacy rows retain history-based provenance.
    if (liveHash(binary.executable_path) !== binary.sha256?.toUpperCase()) return false;
    return !binary.native_dll_sha256 || liveHash(path.join(path.dirname(binary.executable_path),
        'winblaze_native.dll')) === binary.native_dll_sha256.toUpperCase();
};
// Older evidence remains downloadable but cannot silently enter a new binary's median.
for (const [rows, family] of [[data.featurePerformance,null],[data.workflow,null],[data.competitors,null],[data.elevated,null],[data.engine,'WinBlaze engine'],[data.gui,'WinBlaze'],[data.startup,null],[data.uiReady,'WinBlaze'],[data.cache,'WinBlaze'],[data.incremental,'WinBlaze'],[data.native,'WinBlaze native pipeline'],[data.mft,'WinBlaze MFT phases'],[data.publicOperations,'WinBlaze public operations'],[data.indexMemory,'WinBlaze index memory']]) {
    for (const row of rows) {
        const tool = family || row.binary?.tool || row.tool;
        if (!row.binary) {
            const legacy = tool.startsWith('WinBlaze') ? (tool === 'WinBlaze engine' ? data.versions.find(r => r.tool === tool) : data.built[0]) : data.inventory.find(r => r.path?.includes(tool));
            row.binary = {tool, product_version: legacy?.version || null, sha256: legacy?.sha256, provenance: 'joined to recorded inventory; original run did not embed binary identity'};
        }
        row.baseline_current = matchesLiveBinary(row.binary) && (!latest(tool) ||
            (latest(tool).identity_sha256 || latest(tool).sha256) ===
            (row.binary.identity_sha256 || row.binary.sha256));
        if (!row.baseline_current) {
            row.historical_eligible = row.included_in_summary !== false;
            row.included_in_summary = false;
            row.exclusion_reason ||= 'Older binary identity; preserved as historical evidence.';
        }
    }
}
const included=rows=>rows.filter(r=>r.included_in_summary!==false&&r.baseline_current!==false);
data.coverage=data.coverage.map(row=>{
    if(row.comparison==='Administrator / MFT access')return {...row,state:included(data.mft).length?'measured':'partial',detail:`Current WinBlaze native process n=${included(data.native).length}; current raw MFT phases n=${included(data.mft).length}. Earlier competitor elevated exports: WinDirStat n=${included(data.elevated).filter(r=>r.tool==='WinDirStat').length}, WizTree n=${included(data.elevated).filter(r=>r.tool==='WizTree').length}. Completed excludes WinBlaze's deferred snapshot write; native process resources include it. ${data.elevatedStatus.detail||''}`};
    if(row.comparison==='Nonempty dense folders')return {...row,detail:`8,192 files of 256 bytes. Common scan/export runs: ${['WinBlaze','WinDirStat','WizTree'].map(tool=>tool+' n='+included(data.workflow).filter(r=>r.tool===tool&&r.profile==='fanout-large').length).join(', ')}. WinBlaze GUI n=${included(data.gui).filter(r=>r.profile==='fanout-large').length}; engine n=${included(data.engine).filter(r=>r.profile==='fanout-large').length}. Synthetic file and logical-byte counts are validated; older rendering observations remain separate.`};
    if(row.comparison==='Incremental add / modify / remove'){const counts=['add','remove','modify'].map(m=>m+' n='+included(data.incremental).filter(r=>r.mutation===m).length);return {...row,state:included(data.incremental).length>=30?'measured':'partial',detail:`Current-build owned tiny-fixture rescans: ${counts.join(', ')}. Every accepted rescan verifies the expected mutation counts. Competitor incremental workflows were not timed.`};}
    if(row.comparison==='Visible / usable window startup')return {...row,detail:`First-idle hidden-window proxy: ${['WinBlaze','WinDirStat','WizTree'].map(tool=>tool+' n='+included(data.startup).filter(r=>r.tool===tool).length).join(', ')}. WinBlaze full-cache restore observations n=${included(data.cache).length}. Accessibility completion observations are not precise visible/usable-window timings.`};
    return row;
});
data.medals = buildMedalTable(data);
const provenance={source:'benchmarks/runs/'+data.generated.replace(/[:.]/g,'-')+'/report-data.json',recorded_utc:data.generated,batch_started_utc:data.batch.started_utc};
const definitions=read('original-medal-events.json').events;
const liveEvents=retainLiveMedals(read('live-medal-results.json'),definitions,data.medals.measurementEvents,medalEvidence(data,definitions),provenance);
const competitorIdentities = Object.fromEntries(['WinDirStat','WizTree'].map(tool=>[tool,latest(tool)]));
const retained = retainCompetitorBests(read('competitor-best-results.json')?.schema
    ? read('competitor-best-results.json') : null, liveEvents, competitorIdentities,
    {source:'benchmarks/runs/'+data.generated.replace(/[:.]/g,'-')+'/report-data.json',recorded_utc:data.generated,
        batch_started_utc:data.batch.started_utc});
data.latestMedals = data.medals;
data.medals = {...data.medals,...rankMedalEvents(retained.events)};
data.competitorBaselinePolicy = 'WinDirStat and WizTree retain the lowest validated n=10 statistic per medal metric and executable version/hash. Subsequent iterations reuse complete competitor baselines until their version or executable hash changes; missing baselines require collection. WinBlaze is rerun and uses the latest validated batch per metric; pending refreshes retain their dated previous result. Each metric may select a different dated batch. Raw latest observations remain separately dated.';
data.featureMedalCoverage = featureMedalCoverage(data,data.medals.events);
const template = fs.readFileSync(path.join(root, 'report-template.html.template'), 'utf8');
const payload = JSON.stringify(data).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
let html = template.replace('/* BENCHMARK_DATA */', payload);
// Render default results at build time. Script-restricted previews still show
// the complete report; browser JavaScript only enhances it with filtering.
const elements = new Map();
const defaults = {profile: 'full', resource: 'memory', 'tool-filter': 'all'};
for (const [,id] of html.matchAll(/id="([^"]+)"/g)) {
    elements.set(id, {
        innerHTML: '', textContent: '', value: defaults[id] || '', checked: false,
        addEventListener() {},
        insertAdjacentHTML(_position, value) { this.innerHTML += value; }
    });
}
const context = vm.createContext({
    document: { getElementById(id) {
        if (!elements.has(id)) throw new Error(`Unknown report element: ${id}`);
        return elements.get(id);
    } }, Date, console, Blob, URL, setTimeout
});
vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], context);
const escape = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[c]));
// These template slots are empty before generation, so replacing the opening
// tag plus immediate close cannot interfere with nested chart markup.
for (const [id, element] of elements) {
    const content = element.innerHTML || (element.textContent ? escape(element.textContent) : '');
    if (content) {
        const slot = new RegExp(`(<(div|span|tbody)\\b[^>]*\\bid="${id}"[^>]*>)(</\\2>)`);
        html = html.replace(slot, (_match, opening, _tag, closing) => opening + content + closing);
    }
}
html = html.replace('<div class="controls"><label for="profile">', '<noscript><p class="notice">Results below are embedded in this file. Enable JavaScript in a browser to use the interactive filters and evidence download.</p></noscript><div class="controls"><label for="profile">');
// Windows previews/indexers can briefly hold the report open without sharing writes.
async function writeWithRetry(filename, content) {
    for (let attempt=0;;attempt++) {
        try { fs.writeFileSync(filename, content); return; }
        catch (error) {
            if (attempt>=19 || !['EBUSY','EACCES','EPERM','UNKNOWN'].includes(error.code)) throw error;
            await new Promise(resolve=>setTimeout(resolve,50));
        }
    }
}
const archive = path.join(root, 'runs', data.generated.replace(/[:.]/g, '-'));
fs.mkdirSync(archive, {recursive: true});
fs.writeFileSync(path.join(archive, 'report-data.json'), JSON.stringify(data, null, 2));
const bestPath=path.join(root, 'competitor-best-results.json');
const bestTemporary=bestPath+'.'+process.pid+'.tmp';
await writeWithRetry(bestTemporary, JSON.stringify(retained.store,null,2));
for(let attempt=0;;attempt++) {
    try {fs.renameSync(bestTemporary,bestPath);break;}
    catch(error) {
        if(attempt>=19||!['EBUSY','EACCES','EPERM','UNKNOWN'].includes(error.code))throw error;
        await new Promise(resolve=>setTimeout(resolve,50));
    }
}
const livePath=path.join(root,'live-medal-results.json');
const liveTemporary=livePath+'.'+process.pid+'.tmp';
await writeWithRetry(liveTemporary,JSON.stringify({schema:1,events:retained.events},null,2));
fs.renameSync(liveTemporary,livePath);
await writeWithRetry(path.join(root, 'CURRENT_COMPARISON.html'), html);
console.log(`Rendered CURRENT_COMPARISON.html; archived aggregate evidence in ${archive}`);

await import('./write-summary.mjs?generated='+encodeURIComponent(data.generated));

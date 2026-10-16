import {CORE, PRODUCTS, COMPETITORS} from './products.mjs';
// Feature-support grades exist for the core products only.
const products = CORE;

export function hasEqualFeatureSupport(support) {
    const level = support?.[products[0]];
    return ['full', 'partial', 'none'].includes(level)
        && products.every(tool => support?.[tool] === level);
}

export function hasWinBlazeFeatureAdvantage(support) {
    const scores = {none:0, partial:1, full:2};
    // Unknown comparator support must remain visible for verification.
    return products.every(tool => Number.isFinite(scores[support?.[tool]]))
        && scores[support.WinBlaze] > Math.max(scores[support.WinDirStat], scores[support.WizTree]);
}

export function isParityFeature(support) {
    const sharedGold = support?.WinBlaze === 'full'
        && ['WinDirStat', 'WizTree'].some(tool => support?.[tool] === 'full');
    return !hasEqualFeatureSupport(support) && !hasWinBlazeFeatureAdvantage(support) && !sharedGold;
}

// Linear interpolation (R-7). Never coerce missing telemetry to zero.
export function percentile(values, fraction) {
    if (!Number.isFinite(fraction) || fraction < 0 || fraction > 1) throw new RangeError('Invalid percentile');
    const sorted=values.filter(Number.isFinite).sort((a,b)=>a-b);
    if (!sorted.length) return null;
    const position=(sorted.length-1)*fraction;
    const lower=Math.floor(position), upper=Math.ceil(position);
    return sorted[lower]+(sorted[upper]-sorted[lower])*(position-lower);
}

// Extended events (EXTENDED_MEDALS_PLAN.md) measured by run-extended-comparison.ps1
// with exact kernel counters. Dataset rows count only when files and logical
// bytes match the fixture manifest; live targets have no manifest.
export const EXTENDED_TARGETS = [
    ['million','1-million-file scale'],['deep','Deep long paths'],['dirs500k','500k empty folders'],
    ['sparse50g','Single 50 GiB sparse file'],['hardlinks','Hard-link fixture'],
    ['sparsecompressed','Sparse and compressed files'],['ads','Alternate data streams'],
    ['junctions','Junction maze'],['unicode','Unicode names'],['denied','Access-denied subtree'],
    ['windows','Windows folder'],['profile','User profile'],['nodemodules','node_modules tree'],
    ['gitrepo','Git repository'],['tengb','10 GiB of 1 MiB files'],['lognormal','Lognormal size mix'],
    ['fragmented','Churned folder'],['onedrive','OneDrive cloud placeholders'],
    ['lognormal@cpuload','Lognormal mix under CPU load'],['nodemodules@changing','Files changing during scan'],
    ['lognormal@diskbusy','Scan while disk is busy'],['million@memlimit','1M files under a 1 GiB memory limit']
];
const LIVE_TARGETS = new Set(['windows','profile','gitrepo','onedrive','nodemodules@changing','full']);
const EXTENDED_SCOPE = 'Whole-process startup + scan + CSV export; elapsed is kernel exit-minus-create time (no launcher overhead).';
const FULL_RESOURCE_EVENTS = [
    ['disk bytes read','MiB',r=>r.read_bytes/1048576],
    ['disk bytes written','MiB',r=>r.write_bytes/1048576],
    ['peak handles','handles',r=>r.peak_handles_sampled||null],
    ['peak threads','threads',r=>r.peak_threads_sampled||null],
    ['page faults','faults',r=>r.page_faults],
    ['peak commit','MiB',r=>r.peak_commit_bytes/1048576],
    ['kernel CPU','CPU-s',r=>r.kernel_cpu_ms/1000],
    ['user CPU','CPU-s',r=>r.user_cpu_ms/1000],
    ['memory per million files','MiB',r=>r.files>0?r.peak_working_set_bytes/1048576/(r.files/1e6):null],
    ['context switches','switches',r=>r.context_switches_sampled||null],
    ['package energy','J',r=>r.package_energy_joules||null]
];
function conditionScope(target) {
    const condition = target.split('@')[1];
    return condition==='cpuload'?' Busy-loop processes on half the logical CPUs run throughout.'
        :condition==='diskbusy'?' A sequential 256 MiB write-and-flush loop runs throughout.'
        :condition==='changing'?' A writer creates and deletes files inside the target throughout.'
        :condition==='memlimit'?' Each scanned process is limited to 1 GiB committed memory (Job object).':'';
}
export function extendedRowValid(row) {
    if (row.exit_code !== 0) return false;
    if (!row.manifest) return true;
    // SpaceSniffer exports no logical sizes; its rows are checked on file count.
    const bytesMatch = row.size_precision === 'files-only' || row.logical_bytes === row.manifest.bytes;
    return row.files === row.manifest.files && bytesMatch;
}
// Products whose runs on a target all failed (timed out or exited with an
// error): {eventName: {tool: reason}}. A failure ranks after every measured
// product and earns no medal, instead of reading as "not measured".
export function extendedFailures(rows) {
    const failures = {};
    for (const [target,label] of EXTENDED_TARGETS.concat([['full','Full C: volume']])) {
        const names = target==='full' ? FULL_RESOURCE_EVENTS.map(([metric])=>`Full C: volume · ${metric}`) : [`${label} · scan workflow`];
        for (const tool of PRODUCTS) {
            const own = rows.filter(r=>r.target===target&&r.tool===tool);
            const valid = own.filter(extendedRowValid).length;
            const timedOut = own.find(r=>r.timed_out);
            // A timeout ends a product's runs on a target; with fewer than 10
            // valid runs before it, the cohort is a failure, not "not measured".
            if (!own.length || valid >= 10 || (valid > 0 && !timedOut)) continue;
            const reason = timedOut
                ? `timed out after ${timedOut.timeout_seconds} s`+(valid ? ` on run ${timedOut.run} (${valid} earlier run${valid===1?'':'s'} completed)` : '')
                : own.some(r=>r.exit_code!==0) ? 'exited with an error' : 'output did not match the fixture';
            for (const name of names) (failures[name] ??= {})[tool] = reason;
        }
    }
    return failures;
}
// Cohorts that completed one retry after a timeout (run-extended-comparison.ps1
// -RetryTimedOut): {eventName: {tool: note}}. Their valid runs, even fewer
// than 10, give the product a value instead of a failure.
export function extendedPartials(rows) {
    const partials = {};
    for (const [target,label] of EXTENDED_TARGETS.concat([['full','Full C: volume']])) {
        const names = target==='full' ? FULL_RESOURCE_EVENTS.map(([metric])=>`Full C: volume · ${metric}`) : [`${label} · scan workflow`];
        for (const tool of PRODUCTS) {
            const own = rows.filter(r=>r.target===target&&r.tool===tool);
            const retry = own.find(r=>r.retry_after_timeout&&extendedRowValid(r));
            const valid = own.filter(extendedRowValid).length;
            if (!retry || valid >= 10 || own.some(r=>r.timed_out)) continue;
            const note = `n=${valid}: timed out in the standard protocol, then completed one retry with a ${Math.round(retry.timeout_seconds/60)} min limit`;
            for (const name of names) (partials[name] ??= {})[tool] = note;
        }
    }
    return partials;
}
// Events a product cannot perform (unsupported-events.json: reviewed reasons,
// e.g. no saved-scan reload in a free edition). Shown as "Not supported", no
// medal; a product that does have measurements is never marked.
export function applyUnsupported(event, unsupported) {
    const marked = Object.fromEntries(Object.entries(unsupported?.[event.name] || {})
        .filter(([tool])=>!Number.isFinite(event.times?.[tool]) && !event.failed?.[tool]));
    if (Object.keys(marked).length) event.unsupported = marked;
    return event;
}
export function extendedMedalEvents(rows) {
    const accepted = rows.filter(extendedRowValid);
    const byTool = (target, metric) => Object.fromEntries(PRODUCTS.map(tool=>[tool,
        accepted.filter(r=>r.target===target&&r.tool===tool).map(metric).filter(Number.isFinite)]));
    const events = [];
    for (const [target,label] of EXTENDED_TARGETS) {
        events.push([`${label} · scan workflow`,'ms',byTool(target,r=>r.elapsed_ms),
            EXTENDED_SCOPE+(LIVE_TARGETS.has(target)?' Live folder or live writer; accessible counts can differ by policy.':' Files and logical bytes must match the fixture manifest.')+conditionScope(target),true]);
    }
    {
        for (const [metric,unit,value] of FULL_RESOURCE_EVENTS) {
            events.push([`Full C: volume · ${metric}`,unit,byTool('full',value),
                'Standard-user whole process scan + CSV export of C:. Exact kernel counters at exit; handle, thread and context-switch counts are 250 ms polled lower bounds; package energy integrates the RAPL power counter (package-wide, quiet machine). Export schemas differ, so written bytes include each product\'s own CSV.',true]);
        }
    }
    return events;
}

// GUI events from run-gui-comparison.ps1. Each product's own results-ready
// signal ends the scan timing (see the runner's header).
export const GUI_EVENTS = [
    ['GUI · usable-window startup','ms',r=>r.startup_ms,'Launch with no target until the main window is visible and answers WM_NULL.'],
    ['GUI full drive · scan to results','ms',r=>r.scan_ms,'Launch with C: until the product signals results are ready.'],
    ['GUI full drive · memory with results','MiB',r=>r.working_set_bytes/1048576,'Working set when results are ready.'],
    ['GUI full drive · responsiveness p95','ms',r=>r.responsiveness_p95_ms,'95th percentile WM_NULL round trip, polled every 100 ms during the scan.'],
    ['GUI full drive · exit with results','ms',r=>r.exit_ms,'WM_CLOSE with full-drive results loaded until the process exits.'],
    ['GUI full drive · GDI objects with results','objects',r=>r.gdi_objects,'GDI objects (GetGuiResources) when results are ready.'],
    ['GUI full drive · USER objects with results','objects',r=>r.user_objects,'USER objects (GetGuiResources) when results are ready.'],
    ['GUI full drive · cancel latency','ms',r=>r.cancel_ms,'Product cancel command after 2 s of scanning until it stops showing a scan in progress.']
];
export const GUI_RESCAN_EVENTS = [
    ['GUI lognormal · repeat scan','ms',r=>r.rescan_ms,'Median of 10 full rescans of the 250k-file lognormal dataset in one GUI session.'],
    ['GUI lognormal · memory growth over 10 rescans','MiB',r=>r.memory_growth_bytes/1048576,'Working set after the 10th rescan minus after the first scan.'],
    ['GUI lognormal · refresh after a small change','ms',r=>r.small_change_ms,"One added file, then each product's fastest refresh until done (WinBlaze incremental rescan, WinDirStat Refresh All, WizTree Scan)."]
];
export function guiRescanEvents(rows) {
    return GUI_RESCAN_EVENTS.map(([name,unit,value,scope])=>[name,unit,
        Object.fromEntries(PRODUCTS.map(tool=>[tool,rows.filter(r=>r.tool===tool&&r.target==='lognormal').map(value).filter(Number.isFinite)])),
        'Standard-user GUI. '+scope,true]);
}
// GUI session events (EXTENDED_MEDALS_PLAN.md #4, #7, #53) on the lognormal dataset;
// run-gui-session-comparison.ps1. Each value is optional per row; events need n=10.
export const GUI_SESSION_EVENTS = [
    ['GUI session · time to first result','ms',r=>r.first_result_ms,'Launch with the 250k-file lognormal dataset until the first result row is on screen (WinDirStat directory list, WinBlaze results list; WizTree draws rows only once its scan completes).'],
    ['GUI session · reload saved results','ms',r=>r.reload_ms,"Launch with the product's own saved scan of the 250k-file lognormal dataset until its loaded-results signal (WinDirStat /LoadFrom, WizTree CSV, WinBlaze CSV)."],
    // 0 ms means no redraw was observable (screen not capturable), not an instant redraw.
    ['GUI session · redraw after resize','ms',r=>r.resize_ms>0?r.resize_ms:null,'With the reloaded results shown, resize the window from 1200x800 to 1600x1000 until its screen pixels last change before 400 ms without change (treemap and lists redrawn).']
];
export function guiSessionEvents(rows) {
    return GUI_SESSION_EVENTS.map(([name,unit,value,scope])=>[name,unit,
        Object.fromEntries(PRODUCTS.map(tool=>[tool,rows.filter(r=>r.tool===tool&&r.target==='lognormal').map(value).filter(Number.isFinite)])),
        'Standard-user GUI. '+scope,true]);
}
// Crash-free rate (#70); run-reliability-comparison.ps1. Lower failure share is better.
export const RELIABILITY_EVENT = 'Reliability · failed scans on hostile datasets';
export function reliabilityEvents(rows) {
    return [[RELIABILITY_EVENT,'%',Object.fromEntries(PRODUCTS.map(tool=>[tool,rows.filter(r=>r.tool===tool).map(r=>r.failure_percent).filter(Number.isFinite)])),
        'Standard-user scan/export. Share of 10 scans per run (rotating deep paths, junction maze, access-denied subtree, alternate data streams, Unicode names) that crashed, hung past 120 s, exited non-zero or wrote no root row. Mean over all 100 scans, so a single failure counts.',true,
        values=>values.reduce((sum,value)=>sum+value,0)/values.length]];
}
export function guiMedalEvents(rows) {
    return GUI_EVENTS.map(([name,unit,value,scope])=>[name,unit,
        Object.fromEntries(PRODUCTS.map(tool=>[tool,rows.filter(r=>r.tool===tool&&r.target==='full').map(value).filter(Number.isFinite)])),
        'Standard-user GUI. '+scope,true]);
}

export function featureMedalCoverage(data, medalEvents=[]) {
    const operations=[...new Set(['content-duplicates','scan-csv-fixture','cache-load','cached-csv','extension-summary','json-export','permission-export','filtered-export','treemap-png','csv-import-export','raw-mft-dump',...(data.featurePerformance||[]).map(r=>r.operation)])];
    const metrics=['elapsed_ms','cpu_ms_sampled_lower_bound','peak_working_set_bytes_sampled','peak_private_bytes_sampled','output_bytes','load_ms','operation_ms'];
    const labels={elapsed_ms:'time',cpu_ms_sampled_lower_bound:'CPU',peak_working_set_bytes_sampled:'peak memory',peak_private_bytes_sampled:'private memory'};
    return operations.flatMap(operation=>metrics.map(metric=>{
        const rows=(data.featurePerformance||[]).filter(r=>r.operation===operation&&r.status==='measured'&&r.validation?.verified&&r.included_in_summary!==false&&r.baseline_current!==false);
        const counts=Object.fromEntries(products.map(tool=>[tool,rows.filter(r=>r.tool===tool).length]));
        const event=operation==='scan-csv-fixture'&&labels[metric]?medalEvents.find(e=>e.name===`Feature fixture · scan/CSV ${labels[metric]}`):null;
        let reason;
        if(event) reason='Medals awarded: equivalent scan/CSV boundary, validated n=10 per product. Sampled telemetry counts appear in medal events.';
        else if(metric==='output_bytes') reason='Not ranked: different export schemas, content and PNG compression; smaller output is not necessarily better.';
        else if(metric==='load_ms'||metric==='operation_ms') reason='Not ranked: native phase boundaries are only instrumented for WinBlaze; equivalent competitor phases are unavailable.';
        else if(products.some(tool=>counts[tool]<10)) reason='Not ranked: validated n=10 runs are missing for '+products.filter(tool=>counts[tool]<10).join(', ')+'. Missing measurements receive no medal.';
        else reason='Not ranked: equivalent process boundaries or sufficient resource samples have not been established.';
        return {operation,metric,counts,status:event?'awarded':'not-ranked',awards:event?.awards||[],reason};
    }));
}

// Products without a CSV export run whole-process "scan + export" events on a
// lighter boundary (startup + scan only): their values are shown, not ranked.
export const NO_EXPORT_PRODUCTS = {'TreeSize Free': 'Not ranked: no CSV export, so its run ends after the scan (lighter boundary than scan + export).'};
export function isExportBoundaryEvent(name) {
    return /scan workflow|90th percentile completion|^Full C: volume|^Standard-user|^Administrator|^Feature fixture|^Empty-file scale|^Nonempty dense folder|^Quick folder|^Reliability/.test(name);
}
// Gold-first ordering, then silver, then bronze. Exact ties share a place.
export function rankMedalEvents(events) {
    const standings = PRODUCTS.map(tool => ({tool, gold: 0, silver: 0, bronze: 0}));
    const scored = [];
    for (const event of events) {
        if (event.category === 'feature') {
            if (!isParityFeature(event.support)) continue;
            const supportMedals = {full:'gold', partial:'silver', none:'bronze'};
            const awards = products.flatMap(tool => {
                const support = event.support?.[tool];
                const medal = supportMedals[support];
                if (!medal) return [];
                standings.find(row => row.tool === tool)[medal]++;
                return [{tool, support, medal, place:['gold','silver','bronze'].indexOf(medal)+1}];
            });
            if (awards.length) scored.push({...event, awards});
            continue;
        }
        // Core products must all be measured; other products rank when measured.
        if (!(event.comparable || event.observational) || CORE.some(tool => !Number.isFinite(event.times[tool]))) continue;
        const unranked = isExportBoundaryEvent(event.name)
            ? Object.fromEntries(Object.entries(NO_EXPORT_PRODUCTS).filter(([tool]) => Number.isFinite(event.times[tool])))
            : {};
        const ordered = PRODUCTS.filter(tool => Number.isFinite(event.times[tool]) && !unranked[tool])
            .map(tool => ({tool, ms: event.times[tool]})).sort((a,b) => a.ms-b.ms);
        const awards = ordered.map((entry, index) => {
            const place = ordered.findIndex(other => other.ms === entry.ms) + 1;
            const medal = ['gold', 'silver', 'bronze'][place-1] ?? null;
            if (medal) standings.find(row => row.tool === entry.tool)[medal]++;
            return {...entry, place, medal};
        });
        // Failed products share the place after every measured product; no medal.
        for (const tool of Object.keys(event.failed || {})) {
            awards.push({tool, ms: null, place: ordered.length + 1, medal: null, failed: event.failed[tool]});
        }
        for (const [tool, reason] of Object.entries(unranked)) {
            awards.push({tool, ms: event.times[tool], place: null, medal: null, unranked: reason});
        }
        scored.push({...event, awards, ...(Object.keys(unranked).length ? {unranked} : {})});
    }
    standings.sort((a,b) => b.gold-a.gold || b.silver-a.silver || b.bronze-a.bronze || a.tool.localeCompare(b.tool));
    standings.forEach((entry,index) => {
        entry.place = standings.findIndex(other => other.gold===entry.gold && other.silver===entry.silver && other.bronze===entry.bronze)+1;
    });
    const relative = event => {
        if (event.category === 'feature') return null;
        const best = Math.min(...COMPETITORS.filter(tool => !event.unranked?.[tool]).map(tool => event.times[tool]).filter(Number.isFinite));
        return best === 0 ? (event.times.WinBlaze === 0 ? 1 : Infinity)
            : event.times.WinBlaze / best;
    };
    // A dimensionless ratio makes CPU, memory and time gaps comparable for
    // prioritization. Keep exact ties stable and nonnumeric support rows last.
    scored.sort((a,b) => {
        const left = relative(a), right = relative(b);
        if (left === right) return 0;
        if (left === null) return 1;
        if (right === null) return -1;
        return left > right ? -1 : 1;
    });
    return {standings, events: scored.map(event => {
        const ratio = relative(event);
        return {...event, relative_ratio: Number.isFinite(ratio) ? ratio : null,
            relative_label: ratio === null ? 'No numeric performance ratio'
                : ratio === Infinity ? 'WinBlaze / best competitor: unbounded (zero measured comparator value)'
                : `WinBlaze / best competitor: ${ratio.toFixed(2)}×`};
    })};
}

export function buildMedalTable(data) {
    const events = [];
    const add = (name, unit, measurements, scope, comparable=false, statistic=values=>percentile(values,0.5), minimum=10) => {
        const times = {}; const samples = {}; const acceptedMeasurements = {};
        for (const tool of PRODUCTS) {
            const values = (measurements[tool] || []).filter(Number.isFinite).sort((a,b)=>a-b);
            acceptedMeasurements[tool] = values;
            samples[tool] = values.length;
            if (values.length>=minimum) {
                const value=statistic(values);
                if (Number.isFinite(value)) times[tool]=value;
            }
        }
        events.push({name, unit, comparable, observational:!comparable, times, samples, scope,
            measurements:acceptedMeasurements});
    };
    const valid = rows => rows.filter(r=>r.included_in_summary!==false && r.baseline_current!==false);
    const gui = profile => valid(data.gui).filter(r=>(r.profile||r.scenario.replace(/-\d+$/,''))===profile);
    const common = profile => valid(data.workflow||[]).filter(r=>r.profile===profile);
    const commonAvailable = profile => common(profile).length > 0;
    const competitor = (tool,profile,elevated=false) => valid(elevated?data.elevated:(commonAvailable(profile)?data.workflow:data.competitors)).filter(r=>r.tool===tool && r.profile===profile);
    add('Startup · first idle message queue','ms',Object.fromEntries(PRODUCTS.map(tool=>[tool,
        valid(data.startup).filter(r=>r.tool===tool&&r.idle_reached).map(r=>r.initialization_ms)])),
        'Same hidden-window first-idle protocol. Does not measure a visible usable window.',true);
    for (const [name,key,unit,divisor] of [
        ['Startup CPU at first idle','cpu_ms_at_idle','CPU-s',1000],
        ['Startup working set at first idle','working_set_bytes_at_idle','MiB',1048576],
        ['Startup peak working set at first idle','peak_working_set_bytes_at_idle','MiB',1048576]
    ]) {
        add(name,unit,Object.fromEntries(PRODUCTS.map(tool=>[tool,
            valid(data.startup).filter(r=>r.tool===tool&&r.idle_reached&&Number.isFinite(r[key])&&r[key]>0).map(r=>r[key]/divisor)])),
            'Same hidden-window first-idle observation boundary; excludes later cache restoration and usable-window work.',true);
    }
    for (const [profile,label] of [['tiny','Quick folder'],['scale','Empty-file scale'],['fanout-large','Nonempty dense folder'],['full','Full C: volume']]) {
        add(`${label} · scan workflow`,'ms',{
            WinBlaze:commonAvailable(profile)?common(profile).filter(r=>r.tool==='WinBlaze').map(r=>r.elapsed_ms):gui(profile).map(r=>r.scan_ms),
            ...Object.fromEntries(COMPETITORS.map(tool=>[tool,competitor(tool,profile).map(r=>r.elapsed_ms)]))
        },commonAvailable(profile)?'Common workflow: startup + scan/indexing + CSV export, no GUI rendering. File counts and scanner policies remain visible in the evidence.':'Mixed scope: WinBlaze app-reported GUI scan duration versus competitor startup + scan + CSV export. Accessible-file counts differ on C:.',commonAvailable(profile));
        if (commonAvailable(profile)) {
            const times=Object.fromEntries(PRODUCTS.map(tool=>[tool,common(profile).filter(r=>r.tool===tool).map(r=>r.elapsed_ms)]));
            add(`${label} · 90th percentile completion`,'ms',times,
                'Common startup + scan + CSV export. R-7 interpolated p90; at least 10 valid repetitions per product. Small-sample tail estimate, not a worst-case guarantee. Live-disk differences remain.',true,values=>percentile(values,0.9),10);
            if (profile!=='full') {
                for (const [metric,unit,key,divisor] of [
                    ['peak memory','MiB','peak_working_set_bytes_sampled',1048576],
                    ['private memory','MiB','peak_private_bytes_sampled',1048576],
                    ['CPU consumption','CPU-s','cpu_ms_sampled_lower_bound',1000]
                ]) {
                    add(`${label} · ${metric}`,unit,Object.fromEntries(PRODUCTS.map(tool=>[tool,
                        common(profile).filter(r=>r.tool===tool&&Number.isFinite(r[key])).map(r=>r[key]/divisor)])),
                        'Same synthetic dataset and startup + scan + CSV export boundary; median of sampled resources. At least 3 valid samples per product; short-run missing samples remain missing.',true);
                }
            }
        }
    }
    const native = valid(data.native);
    add('Administrator · full-drive workflow','ms',{
        WinBlaze:native.map(r=>r.scan_ms),
        ...Object.fromEntries(COMPETITORS.map(tool=>[tool,competitor(tool,'full',true).map(r=>r.elapsed_ms)]))
    },'Mixed scope: WinBlaze native scan + index publication (deferred cache write excluded; no GUI) versus competitor startup + scan + export. Administrator tokens; file totals differ.');
    for (const elevated of [false,true]) {
        for (const [metric,unit,key,guiKey,divisor] of [
            ['peak memory','MiB','peak_working_set_bytes_sampled','sampled_peak_working_set_bytes_through_observation',1048576],
            ['private memory','MiB','peak_private_bytes_sampled','sampled_peak_private_bytes_through_observation',1048576],
            ['CPU consumption','CPU-s','cpu_ms_sampled_lower_bound','sampled_cpu_ms_through_observation',1000]
        ]) {
            const values = rows => rows.filter(r=>r[key]!=null).map(r=>r[key]/divisor);
            add(`${elevated?'Administrator':'Standard-user'} · full-drive ${metric}`,unit,{
                WinBlaze:elevated?values(native):commonAvailable('full')?values(common('full').filter(r=>r.tool==='WinBlaze')):gui('full').filter(r=>r[guiKey]!=null).map(r=>r[guiKey]/divisor),
                ...Object.fromEntries(COMPETITORS.map(tool=>[tool,values(competitor(tool,'full',elevated))]))
            },!elevated&&commonAvailable('full')?'Common whole-process scan/export telemetry; sampled resources, lower wins.':elevated?'Mixed scope: whole native process including deferred snapshot write and tree queries versus whole competitor export process. Sampled resources; lower wins.':
                'Mixed scope: WinBlaze GUI scan through later ready observation versus whole competitor export process. Sampled resources; lower wins.',!elevated&&commonAvailable('full'));
        }
    }
    for (const feature of data.features?.rows || []) {
        events.push({category:'feature', name:feature.feature, support:feature.support,
            scope:(feature.criterion ? feature.criterion + ' ' : '') + feature.note + ' Source/interface verification is distinct from a live runtime pass. Unverified support receives no medal.', sources:feature.sources});
    }
    const featureRows = valid(data.featurePerformance || []).filter(r=>r.operation==='scan-csv-fixture'&&r.status==='measured'&&r.validation?.verified);
    if (featureRows.length) {
        for (const [label,unit,key,divisor] of [
            ['time','ms','elapsed_ms',1],
            ['CPU','CPU-s','cpu_ms_sampled_lower_bound',1000],
            ['peak memory','MiB','peak_working_set_bytes_sampled',1048576],
            ['private memory','MiB','peak_private_bytes_sampled',1048576]
        ]) {
            add(`Feature fixture · scan/CSV ${label}`,unit,Object.fromEntries(PRODUCTS.map(tool=>[tool,
                featureRows.filter(r=>r.tool===tool&&r[key]!=null).map(r=>r[key]/divisor)])),
                'Common whole-process scan/export of the same 224-file, 224 MiB fixture; includes native indexing for WinBlaze. Output counts validated.',true);
        }
    }
    for (const event of extendedMedalEvents(valid(data.extended || []))) add(...event);
    const failures = extendedFailures(valid(data.extended || []));
    const partials = extendedPartials(valid(data.extended || []));
    for (const event of events) {
        for (const [tool, note] of Object.entries(partials[event.name] || {})) {
            const values = event.measurements?.[tool] || [];
            if (Number.isFinite(event.times[tool]) || !values.length) continue;
            event.times[tool] = percentile(values, 0.5);
            (event.partial ??= {})[tool] = note;
        }
        // A product with valid measurements never counts as failed.
        const failed = Object.fromEntries(Object.entries(failures[event.name] || {}).filter(([tool])=>!Number.isFinite(event.times[tool])));
        if (Object.keys(failed).length) event.failed = failed;
        applyUnsupported(event, data.unsupported);
    }
    for (const event of guiMedalEvents(valid(data.guiMedals || []))) add(...event);
    for (const event of guiRescanEvents(valid(data.guiRescan || []))) add(...event);
    for (const event of guiSessionEvents(valid(data.guiSession || []))) add(...event);
    for (const event of reliabilityEvents(valid(data.reliability || []))) add(...event);
    // Every created event is listed, even before its first measurement.
    const footprint = valid(data.footprint || []);
    add('Install footprint · 64-bit GUI files','MiB',Object.fromEntries(PRODUCTS.map(tool=>[tool,
        footprint.filter(r=>r.tool===tool).map(r=>r.bytes/1048576)])),
        'Bytes of the files each product needs to run its 64-bit GUI (measure-footprint.ps1); optional languages and 32-bit builds excluded.',true);
    return {...rankMedalEvents(events), measurementEvents:events,
        exclusions:[
            {name:'Features with equal support', reason:'No feature medals when all three products have the same verified support grade. Equal comparison rows are omitted; equal grades do not imply identical implementations. Excluded: '+(data.features?.rows||[]).filter(row=>hasEqualFeatureSupport(row.support)).map(row=>row.feature).join(', ')+'.'},
            {name:'Raw MFT phases and directory-walk engine', reason:'Component measurements remain in the report, but are not extra medals for the same product/workload.'},
            {name:'Cache, incremental updates and input latency', reason:'Three-run measurements are missing for one or more products; missing values never receive a bronze medal.'},
            {name:'Correctness, allocation and treemap rendering', reason:'Reported separately as correctness/behavior findings; fastest or lowest is not necessarily correct.'}
            ,{name:'Product-specific feature performance', reason:'Timings/resources remain visible; workflows supported by fewer than three products or with different boundaries do not earn performance medals. Feature-support medals apply only to retained parity differences.'}
        ]};
}

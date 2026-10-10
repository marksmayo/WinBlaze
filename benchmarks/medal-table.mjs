const products = ['WinBlaze', 'WinDirStat', 'WizTree'];

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

export function timingVariability(values) {
    const middle=percentile(values,0.5);
    return middle>0?(percentile(values,0.75)-percentile(values,0.25))/middle*100:null;
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

// Gold-first ordering, then silver, then bronze. Exact ties share a place.
export function rankMedalEvents(events) {
    const standings = products.map(tool => ({tool, gold: 0, silver: 0, bronze: 0}));
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
        if (!(event.comparable || event.observational) || products.some(tool => !Number.isFinite(event.times[tool]))) continue;
        const ordered = products.map(tool => ({tool, ms: event.times[tool]})).sort((a,b) => a.ms-b.ms);
        const awards = ordered.map((entry, index) => {
            const place = ordered.findIndex(other => other.ms === entry.ms) + 1;
            const medal = ['gold', 'silver', 'bronze'][place-1];
            standings.find(row => row.tool === entry.tool)[medal]++;
            return {...entry, place, medal};
        });
        scored.push({...event, awards});
    }
    standings.sort((a,b) => b.gold-a.gold || b.silver-a.silver || b.bronze-a.bronze || a.tool.localeCompare(b.tool));
    standings.forEach((entry,index) => {
        entry.place = standings.findIndex(other => other.gold===entry.gold && other.silver===entry.silver && other.bronze===entry.bronze)+1;
    });
    const relative = event => {
        if (event.category === 'feature') return null;
        const best = Math.min(event.times.WinDirStat, event.times.WizTree);
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
        for (const tool of products) {
            const values = measurements[tool].filter(Number.isFinite).sort((a,b)=>a-b);
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
    add('Startup · first idle message queue','ms',Object.fromEntries(products.map(tool=>[tool,
        valid(data.startup).filter(r=>r.tool===tool&&r.idle_reached).map(r=>r.initialization_ms)])),
        'Same hidden-window first-idle protocol. Does not measure a visible usable window.',true);
    for (const [name,key,unit,divisor] of [
        ['Startup CPU at first idle','cpu_ms_at_idle','CPU-s',1000],
        ['Startup working set at first idle','working_set_bytes_at_idle','MiB',1048576],
        ['Startup peak working set at first idle','peak_working_set_bytes_at_idle','MiB',1048576]
    ]) {
        add(name,unit,Object.fromEntries(products.map(tool=>[tool,
            valid(data.startup).filter(r=>r.tool===tool&&r.idle_reached&&Number.isFinite(r[key])&&r[key]>0).map(r=>r[key]/divisor)])),
            'Same hidden-window first-idle observation boundary; excludes later cache restoration and usable-window work.',true);
    }
    for (const [profile,label] of [['tiny','Quick folder'],['scale','Empty-file scale'],['fanout-large','Nonempty dense folder'],['full','Full C: volume']]) {
        add(`${label} · scan workflow`,'ms',{
            WinBlaze:commonAvailable(profile)?common(profile).filter(r=>r.tool==='WinBlaze').map(r=>r.elapsed_ms):gui(profile).map(r=>r.scan_ms),
            WinDirStat:competitor('WinDirStat',profile).map(r=>r.elapsed_ms),
            WizTree:competitor('WizTree',profile).map(r=>r.elapsed_ms)
        },commonAvailable(profile)?'Common workflow: startup + scan/indexing + CSV export, no GUI rendering. File counts and scanner policies remain visible in the evidence.':'Mixed scope: WinBlaze app-reported GUI scan duration versus competitor startup + scan + CSV export. Accessible-file counts differ on C:.',commonAvailable(profile));
        if (commonAvailable(profile)) {
            const times=Object.fromEntries(products.map(tool=>[tool,common(profile).filter(r=>r.tool===tool).map(r=>r.elapsed_ms)]));
            add(`${label} · 90th percentile completion`,'ms',times,
                'Common startup + scan + CSV export. R-7 interpolated p90; at least 10 valid repetitions per product. Small-sample tail estimate, not a worst-case guarantee. Live-disk differences remain.',true,values=>percentile(values,0.9),10);
            add(`${label} · timing variability`,'%',times,
                'Common startup + scan + CSV export. Interquartile range / median × 100; lower means more consistent, not faster. At least 10 valid repetitions; uncontrolled caches and live-disk changes affect spread.',true,timingVariability,10);
            if (profile!=='full') {
                for (const [metric,unit,key,divisor] of [
                    ['peak memory','MiB','peak_working_set_bytes_sampled',1048576],
                    ['private memory','MiB','peak_private_bytes_sampled',1048576],
                    ['CPU consumption','CPU-s','cpu_ms_sampled_lower_bound',1000]
                ]) {
                    add(`${label} · ${metric}`,unit,Object.fromEntries(products.map(tool=>[tool,
                        common(profile).filter(r=>r.tool===tool&&Number.isFinite(r[key])).map(r=>r[key]/divisor)])),
                        'Same synthetic dataset and startup + scan + CSV export boundary; median of sampled resources. At least 3 valid samples per product; short-run missing samples remain missing.',true);
                }
            }
        }
    }
    const native = valid(data.native);
    add('Administrator · full-drive workflow','ms',{
        WinBlaze:native.map(r=>r.scan_ms),
        WinDirStat:competitor('WinDirStat','full',true).map(r=>r.elapsed_ms),
        WizTree:competitor('WizTree','full',true).map(r=>r.elapsed_ms)
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
                WinDirStat:values(competitor('WinDirStat','full',elevated)),
                WizTree:values(competitor('WizTree','full',elevated))
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
            add(`Feature fixture · scan/CSV ${label}`,unit,Object.fromEntries(products.map(tool=>[tool,
                featureRows.filter(r=>r.tool===tool&&r[key]!=null).map(r=>r[key]/divisor)])),
                'Common whole-process scan/export of the same 224-file, 224 MiB fixture; includes native indexing for WinBlaze. Output counts validated.',true);
        }
    }
    return {...rankMedalEvents(events), measurementEvents:events,
        exclusions:[
            {name:'Features with equal support', reason:'No feature medals when all three products have the same verified support grade. Equal comparison rows are omitted; equal grades do not imply identical implementations. Excluded: '+(data.features?.rows||[]).filter(row=>hasEqualFeatureSupport(row.support)).map(row=>row.feature).join(', ')+'.'},
            {name:'Raw MFT phases and directory-walk engine', reason:'Component measurements remain in the report, but are not extra medals for the same product/workload.'},
            {name:'Cache, incremental updates and input latency', reason:'Three-run measurements are missing for one or more products; missing values never receive a bronze medal.'},
            {name:'Correctness, allocation and treemap rendering', reason:'Reported separately as correctness/behavior findings; fastest or lowest is not necessarily correct.'}
            ,{name:'Product-specific feature performance', reason:'Timings/resources remain visible; workflows supported by fewer than three products or with different boundaries do not earn performance medals. Feature-support medals apply only to retained parity differences.'}
        ]};
}

import {EXTENDED_TARGETS} from './medal-table.mjs';
import {PRODUCTS as products} from './products.mjs';
const matches=(a,b)=>['name','unit','scope','comparable','observational'].every(key=>a[key]===b[key]);

export function retainLiveMedals(store,definitions,current,identities,provenance) {
    if(store && store.schema!==1)throw new Error('Unsupported live medal schema');
    return definitions.map(definition=>{
        const previous=store?.events.find(event=>matches(event,definition));
        const candidate=current.find(event=>matches(event,definition));
        const event={...definition,times:{...previous?.times},samples:{...previous?.samples},
            measurements:{...previous?.measurements},result_sources:{...previous?.result_sources},
            competitor_bests:{...previous?.competitor_bests},partial:{...previous?.partial},pending_refresh:{}};
        for(const tool of products) {
            const evidence=identities[definition.name]?.[tool];
            const values=candidate?.measurements?.[tool];
            const fresh=evidence && Date.parse(evidence.measurement_utc)>=Date.parse(provenance.batch_started_utc);
            // A retried cohort (medal-table.mjs extendedPartials) carries fewer
            // than 10 runs; it is kept with its note until a full cohort replaces it.
            const partial=candidate?.partial?.[tool];
            const count=partial?values?.length:10;
            if(fresh && count>0 && candidate.samples[tool]===count && values?.length===count
                && values.every(Number.isFinite) && Number.isFinite(candidate.times[tool])) {
                event.times[tool]=candidate.times[tool];
                event.samples[tool]=count;
                event.measurements[tool]=[...values];
                event.result_sources[tool]={...provenance,...evidence};
                if(partial)event.partial[tool]=partial; else delete event.partial[tool];
                event.pending_refresh[tool]=false;
            } else {
                event.pending_refresh[tool]=true;
            }
        }
        return event;
    });
}

// Extended events name their dataset label or the full-drive resource family.
function extendedTarget(name){
    const target=EXTENDED_TARGETS.find(([,label])=>name===label+' · scan workflow');
    if(target)return target[0];
    return /^Full C: volume · (disk bytes|peak handles|peak threads|page faults|peak commit|kernel CPU|user CPU|memory per million files|context switches|package energy)/.test(name)?'full':null;
}
export function medalEvidence(data,definitions) {
    const labels={'Quick folder':'tiny','Empty-file scale':'scale','Nonempty dense folder':'fanout-large','Full C: volume':'full'};
    return Object.fromEntries(definitions.map(event=>[event.name,Object.fromEntries(products.map(tool=>{
        let rows;
        if(event.name.startsWith('Startup'))rows=(data.startup||[]).filter(r=>r.tool===tool&&r.idle_reached);
        else if(event.name.startsWith('Administrator'))rows=tool==='WinBlaze'?(data.native||[]):(data.elevated||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('Install footprint'))rows=(data.footprint||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('Reliability'))rows=(data.reliability||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('GUI session'))rows=(data.guiSession||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('GUI lognormal'))rows=(data.guiRescan||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('GUI'))rows=(data.guiMedals||[]).filter(r=>r.tool===tool);
        else if(extendedTarget(event.name))rows=(data.extended||[]).filter(r=>r.tool===tool&&r.target===extendedTarget(event.name));
        else if(event.name.startsWith('Feature fixture'))rows=(data.featurePerformance||[]).filter(r=>r.tool===tool&&r.operation==='scan-csv-fixture'&&r.status==='measured'&&r.validation?.verified);
        else {const profile=event.name.startsWith('Standard-user')?'full':Object.entries(labels).find(([label])=>event.name.startsWith(label))?.[1];rows=(data.workflow||[]).filter(r=>r.tool===tool&&r.profile===profile);}
        rows=rows.filter(r=>r.included_in_summary!==false&&r.baseline_current!==false);
        const dates=rows.map(r=>Date.parse(r.started_utc||r.utc||r.binary?.observed_utc));
        const measured=dates.length&&dates.every(Number.isFinite)?new Date(Math.min(...dates)).toISOString():null;
        return [tool,{identity:rows.at(-1)?.binary,measurement_utc:measured}];
    }))]));
}

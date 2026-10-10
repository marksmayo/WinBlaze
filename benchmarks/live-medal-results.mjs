const products=['WinBlaze','WinDirStat','WizTree'];
const matches=(a,b)=>['name','unit','scope','comparable','observational'].every(key=>a[key]===b[key]);

export function retainLiveMedals(store,definitions,current,identities,provenance) {
    if(store && store.schema!==1)throw new Error('Unsupported live medal schema');
    return definitions.map(definition=>{
        const previous=store?.events.find(event=>matches(event,definition));
        const candidate=current.find(event=>matches(event,definition));
        const event={...definition,times:{...previous?.times},samples:{...previous?.samples},
            measurements:{...previous?.measurements},result_sources:{...previous?.result_sources},
            competitor_bests:{...previous?.competitor_bests},pending_refresh:{}};
        for(const tool of products) {
            const evidence=identities[definition.name]?.[tool];
            const values=candidate?.measurements?.[tool];
            const fresh=evidence && Date.parse(evidence.measurement_utc)>=Date.parse(provenance.batch_started_utc);
            if(fresh && candidate.samples[tool]===10 && values?.length===10
                && values.every(Number.isFinite) && Number.isFinite(candidate.times[tool])) {
                event.times[tool]=candidate.times[tool];
                event.samples[tool]=10;
                event.measurements[tool]=[...values];
                event.result_sources[tool]={...provenance,...evidence};
                event.pending_refresh[tool]=false;
            } else {
                event.pending_refresh[tool]=true;
            }
        }
        return event;
    });
}

export function medalEvidence(data,definitions) {
    const labels={'Quick folder':'tiny','Empty-file scale':'scale','Nonempty dense folder':'fanout-large','Full C: volume':'full'};
    return Object.fromEntries(definitions.map(event=>[event.name,Object.fromEntries(products.map(tool=>{
        let rows;
        if(event.name.startsWith('Startup'))rows=(data.startup||[]).filter(r=>r.tool===tool&&r.idle_reached);
        else if(event.name.startsWith('Administrator'))rows=tool==='WinBlaze'?(data.native||[]):(data.elevated||[]).filter(r=>r.tool===tool);
        else if(event.name.startsWith('Feature fixture'))rows=(data.featurePerformance||[]).filter(r=>r.tool===tool&&r.operation==='scan-csv-fixture'&&r.status==='measured'&&r.validation?.verified);
        else {const profile=event.name.startsWith('Standard-user')?'full':Object.entries(labels).find(([label])=>event.name.startsWith(label))?.[1];rows=(data.workflow||[]).filter(r=>r.tool===tool&&r.profile===profile);}
        rows=rows.filter(r=>r.included_in_summary!==false&&r.baseline_current!==false);
        const dates=rows.map(r=>Date.parse(r.started_utc||r.utc||r.binary?.observed_utc));
        const measured=dates.length&&dates.every(Number.isFinite)?new Date(Math.min(...dates)).toISOString():null;
        return [tool,{identity:rows.at(-1)?.binary,measurement_utc:measured}];
    }))]));
}

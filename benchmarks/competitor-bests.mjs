export function retainCompetitorBests(store, events, identities, provenance) {
    if (store && store.schema !== 1) throw new Error('Unsupported competitor best schema');
    const entries = new Map((store?.entries || []).map(entry => [entry.key, entry]));
    const selected = events.map(original => {
        const event = {...original, times:{...original.times}, samples:{...original.samples},
            measurements:{...original.measurements}, competitor_bests:{...original.competitor_bests}, pending_refresh:{...original.pending_refresh}};
        for (const tool of ['WinDirStat','WizTree']) {
            const identity = identities[tool];
            if (!identity?.sha256) continue;
            const key = JSON.stringify([tool,event.name,event.unit,event.scope,event.comparable,
                event.observational,identity.product_version,identity.file_version,identity.sha256.toUpperCase()]);
            let best = entries.get(key);
            const values = original.measurements?.[tool];
            const value = original.times?.[tool];
            if (!original.pending_refresh?.[tool] && original.samples?.[tool] === 10 && values?.length === 10
                && values.every(Number.isFinite) && Number.isFinite(value)
                && (!best || value < best.value)) {
                best = {key,tool,name:event.name,unit:event.unit,scope:event.scope,
                    identity:{...identity},value,samples:10,values:[...values],...provenance};
                entries.set(key,best);
            }
            if (best) {
                event.times[tool] = best.value;
                event.samples[tool] = best.samples;
                event.measurements[tool] = [...best.values];
                event.competitor_bests[tool] = best;
                event.pending_refresh[tool] = false;
            }
        }
        return event;
    });
    return {store:{schema:1,entries:[...entries.values()]},events:selected};
}

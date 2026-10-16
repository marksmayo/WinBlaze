// Products compared by the report. CORE products must all have a statistic
// before an event is ranked (a missing core result keeps the event pending);
// the other products join an event's ranking whenever they have n=10 results
// and are otherwise listed as not measured for it (unsupported or not run).
export const CORE = ['WinBlaze', 'WinDirStat', 'WizTree'];
export const PRODUCTS = [...CORE, 'TreeSize Free', 'SpaceSniffer', 'Everything', 'Directory Report'];
export const COMPETITORS = PRODUCTS.filter(tool => tool !== 'WinBlaze');

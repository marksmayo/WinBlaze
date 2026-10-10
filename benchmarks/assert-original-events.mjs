import assert from 'node:assert/strict';

export function assertOriginalEvents(events, original) {
    assert.equal(events.length, original.length, 'All original categories require evidence');
    const names = new Set(events.map(event => event.name));
    assert.equal(names.size, events.length, 'Original categories must not be duplicated');
    for (const event of original) {
        assert(names.has(event.name), `Missing original category: ${event.name}`);
        const current = events.find(candidate => candidate.name === event.name);
        for (const key of ['unit', 'scope', 'comparable', 'observational']) {
            assert.equal(current[key], event[key], `Changed original definition: ${event.name} ${key}`);
        }
    }
}

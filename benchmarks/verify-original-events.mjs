import fs from 'node:fs';
import assert from 'node:assert/strict';
import { assertOriginalEvents } from './assert-original-events.mjs';

const original = JSON.parse(fs.readFileSync(new URL('./original-medal-events.json', import.meta.url))).events;
const renamed = structuredClone(original);
renamed[0].name = 'Replacement event';
assert.throws(() => assertOriginalEvents(renamed, original), /original|category/i,
    'Replacing an event must not satisfy the original goal');
const changedUnit = structuredClone(original);
changedUnit[0].unit = 'replacement unit';
assert.throws(() => assertOriginalEvents(changedUnit, original), /original|definition/i,
    'Changing units must not satisfy the original goal');
const changedScope = structuredClone(original);
changedScope[0].scope = 'Replacement boundary';
assert.throws(() => assertOriginalEvents(changedScope, original), /original|definition/i);
assertOriginalEvents([...original].reverse(), original);
assert.throws(() => assertOriginalEvents(original.slice(1), original), /original/i);
const duplicated = structuredClone(original);
duplicated[1] = duplicated[0];
assert.throws(() => assertOriginalEvents(duplicated, original), /original/i);
console.log('PASS: original event names, units, scopes and comparison flags remain unchanged; display order may change.');

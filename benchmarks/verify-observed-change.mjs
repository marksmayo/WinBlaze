import assert from 'node:assert/strict';
import { formatObservedChange } from './format-observed-change.mjs';

assert.equal(formatObservedChange(-40.5, 'time'), '40.5% higher observed time',
    'A measured slowdown must be described as higher time');
assert.equal(formatObservedChange(12.34, 'time'), '12.3% lower observed time');
console.log('PASS: observed changes preserve direction and round to one decimal.');

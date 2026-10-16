// Append extended and GUI medal events to original-medal-events.json once all
// three products have a complete n=10 statistic in the rendered report.
// Usage: node benchmarks/adopt-medal-events.mjs [--dry-run]
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {CORE} from './products.mjs';
import {EXTENDED_TARGETS, GUI_EVENTS, GUI_RESCAN_EVENTS, GUI_SESSION_EVENTS, RELIABILITY_EVENT} from './medal-table.mjs';

const root = path.dirname(fileURLToPath(import.meta.url));
const definitionsPath = path.join(root, 'original-medal-events.json');
const html = fs.readFileSync(path.join(root, 'CURRENT_COMPARISON.html'), 'utf8');
const data = JSON.parse(html.match(/const DATA = (.*);/)[1]);
const raw = fs.readFileSync(definitionsPath, 'utf8');
const bom = raw.charCodeAt(0) === 0xFEFF;
const definitions = JSON.parse(raw.replace(/^﻿/, ''));
const known = new Set(definitions.events.map(e => e.name));
const candidate = name => EXTENDED_TARGETS.some(([, label]) => name === `${label} · scan workflow`)
    || /^Full C: volume · (disk bytes|peak handles|peak threads|page faults|peak commit|kernel CPU|user CPU|memory per million files|context switches|package energy)/.test(name)
    || GUI_EVENTS.some(([eventName]) => name === eventName)
    || GUI_RESCAN_EVENTS.some(([eventName]) => name === eventName)
    || GUI_SESSION_EVENTS.some(([eventName]) => name === eventName)
    || name === RELIABILITY_EVENT
    || name === 'Install footprint · 64-bit GUI files'
    || name === 'Administrator · full-drive private memory';
// Adoption needs every core product; other products join when measured.
const products = CORE;
const adopted = [];
for (const event of data.latestMedals.measurementEvents) {
    if (known.has(event.name) || !candidate(event.name)) continue;
    if (!products.every(tool => event.samples[tool] === 10 && Number.isFinite(event.times[tool]))) continue;
    definitions.events.push({name: event.name, unit: event.unit, scope: event.scope,
        comparable: event.comparable, observational: event.observational});
    adopted.push(event.name);
}
console.log(`${adopted.length} events adopted; ${definitions.events.length} official categories`);
for (const name of adopted) console.log('  + ' + name);
if (!process.argv.includes('--dry-run') && adopted.length) {
    // Keep the file's \uXXXX escaping so diffs show only the appended events.
    const out = JSON.stringify(definitions, null, 2)
        .replace(/[\u0080-￿]/g, c => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
    fs.writeFileSync(definitionsPath, (bom ? '﻿' : '') + out + (raw.endsWith('\n') ? '\n' : ''));
}

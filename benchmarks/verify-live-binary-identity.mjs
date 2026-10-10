import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';

const html = fs.readFileSync(new URL('./CURRENT_COMPARISON.html', import.meta.url), 'utf8');
const data = JSON.parse(html.match(/const DATA = (.*);/)[1]);
const hashes = new Map();
for (const rows of [data.native, data.mft, data.featurePerformance, data.engine, data.gui]) {
    for (const row of rows) {
        const binary = row.binary;
        if (!binary?.executable_path || !fs.existsSync(binary.executable_path)) continue;
        if (!hashes.has(binary.executable_path)) {
            hashes.set(binary.executable_path, crypto.createHash('sha256')
                .update(fs.readFileSync(binary.executable_path)).digest('hex').toUpperCase());
        }
        if (hashes.get(binary.executable_path) !== binary.sha256.toUpperCase()) {
            assert.equal(row.baseline_current, false,
                `${binary.tool}: replaced executable must be excluded even before another measurement records its new version`);
        }
    }
}
console.log('PASS: replaced executables cannot enter current medians through stale version history');

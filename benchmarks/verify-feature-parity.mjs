import fs from 'node:fs';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const root = new URL('./', import.meta.url);
const data = JSON.parse(fs.readFileSync(new URL('feature-comparison.json', root), 'utf8'));
const tools = ['WinBlaze', 'WinDirStat', 'WizTree'];
assert(data.rows.length >= 40, 'Detailed audit must expose subfeature differences');
assert.equal(new Set(data.rows.map(row => row.feature)).size, data.rows.length);
for (const row of data.rows) {
    assert(row.criterion, row.feature + ': explicit scoring scope required');
    for (const tool of tools) {
        assert(['full', 'partial', 'none', 'unknown'].includes(row.support[tool]));
        const evidence = row.verification?.[tool];
        assert(evidence?.basis && evidence.details && evidence.sources?.length,
            row.feature + ': missing ' + tool + ' verification evidence');
        assert(evidence.sources.every(id => data.sources.some(source => source.id === id)));
        if (row.support[tool] === 'unknown') assert(evidence.remaining,
            'Unresolved status needs an explicit verification limit');
    }
    assert.equal(row.gap, ['partial', 'none'].includes(row.support.WinBlaze));
}
const mtp = data.rows.find(row => row.feature === 'MTP phone/device scanning');
if (mtp.support.WinBlaze === 'full') {
    const scans = JSON.parse(fs.readFileSync(new URL('current-portable-device-results.json', root), 'utf8').replace(/^\uFEFF/, ''));
    assert.equal(scans.length, 10, 'Full MTP verification requires a complete live n=10 batch');
    assert.equal(new Set(scans.map(row => row.run)).size, 10);
    for (const scan of scans) {
        assert.equal(scan.requested_runs, 10);
        assert.equal(scan.status, 'measured');
        assert(scan.validation.verified && scan.validation.files > 0);
        assert.equal(crypto.createHash('sha256').update(fs.readFileSync(scan.binary.executable_path)).digest('hex').toUpperCase(), scan.binary.sha256);
    }
    assert.equal(mtp.verification.WinBlaze.runtime_status, 'live-n10-validated');
    assert(mtp.verification.WinBlaze.remaining, 'Live scans must retain completeness limits');
} else {
    assert.equal(mtp.support.WinBlaze, 'partial');
}
assert.equal(data.rows.find(row => row.feature === 'Regex search').support.WinBlaze, 'full');
assert.equal(data.rows.find(row => row.feature === 'Search and size/date filters').support.WinBlaze, 'full');
assert.equal(data.rows.find(row => row.feature === 'Automatic file-system change monitoring').support.WizTree, 'partial');
assert(data.version_scope.includes('2.8.0'));
const interop = JSON.parse(fs.readFileSync(new URL('current-parity-interop-validation.json', root), 'utf8').replace(/^\uFEFF/, ''));
assert.equal(interop.runs, 10);
assert.equal(interop.records.length, 90);
for (const name of new Set(interop.records.map(row => row.case))) {
    const rows = interop.records.filter(row => row.case === name);
    assert.equal(rows.length, 10);
    assert.equal(new Set(rows.map(row => row.run)).size, 10);
}
assert(interop.records.filter(row => row.case === 'WizTree local timestamp import').every(row => row.delta_seconds === 0));
assert(interop.records.filter(row => row.case === 'WizTree utc timestamp import').every(row => row.delta_seconds === 0));
for (const binary of interop.identities) {
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(binary.path)).digest('hex').toUpperCase(), binary.sha256);
}
console.log(`PASS: ${data.rows.length} scoped capabilities, per-product evidence, real gaps and explicit verification limits.`);

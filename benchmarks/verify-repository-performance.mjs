import fs from 'node:fs';
import assert from 'node:assert/strict';
const root = new URL('./', import.meta.url);
const audit = JSON.parse(fs.readFileSync(new URL('repository-performance-audit.json', root), 'utf8'));
assert.equal(audit.rows.length, 200);
assert.equal(new Set(audit.rows.map(r => r.candidate)).size, 200);
assert.equal(audit.rows.filter(r => r.status === 'implemented').length, 50);
for (const [index, row] of audit.rows.entries()) {
    assert.equal(row.rank, index + 1);
    assert(row.estimated_savings && row.risk && row.evidence);
    if (row.status === 'implemented') {
        assert(fs.existsSync(new URL('../' + row.source, root)), row.source);
        assert.notEqual(row.evidence, 'pending');
    }
}
const html = fs.readFileSync(new URL('CURRENT_COMPARISON.html', root), 'utf8').replace(/<script>[\s\S]*?<\/script>/g, '');
assert(!html.includes('id="repository-performance-audit"'),'Engineering history stays outside the focused comparison');
console.log('PASS: standalone ranked audit retained; comparison omits engineering history.');

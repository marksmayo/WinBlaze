import fs from 'node:fs';
import assert from 'node:assert/strict';
const html=fs.readFileSync(new URL('./CURRENT_COMPARISON.html',import.meta.url),'utf8');
const audit=JSON.parse(fs.readFileSync(new URL('./scan-export-performance-audit.json',import.meta.url),'utf8'));
assert.equal(audit.rows.length,100);
assert.equal(audit.rows.filter(r=>r.status==='implemented').length,7);
const visible=html.replace(/<script>[\s\S]*?<\/script>/g,'');
assert(!visible.includes('id="scan-export-review"'));
console.log('PASS: standalone scan/export evidence retained; comparison omits historical review.');

import fs from 'node:fs';
import assert from 'node:assert/strict';

const html = fs.readFileSync(new URL('./CURRENT_COMPARISON.html', import.meta.url), 'utf8');
const podium = html.match(/id="podium">([\s\S]*?)<div id="medal-events"/);
assert(podium, 'The generated report must contain the podium');
const apps = [...podium[1].matchAll(/<article class="podium-app" style="[^"]*--height:([\d.]+)px" aria-label="([^"]*), rank (\d), (\d+) gold, (\d+) silver, (\d+) bronze"/g)];
assert.equal(apps.length, 3, 'All three products and medal totals must remain visible');
for (const app of apps) {
    assert.equal(Number(app[1]), [240, 180, 125][Number(app[3]) - 1],
        'Podium heights depend on finishing place, independently of medal counts');
}
assert(!html.includes('Podium heights are proportional to gold medals.'));
console.log('PASS: fixed first/second/third podium heights, with product rankings and medal totals retained.');

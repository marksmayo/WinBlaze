// The report must still render when report data predates the `products` field
// (verify-best-report.mjs renders older baselines through this template).
import fs from 'fs';
const template = fs.readFileSync(new URL('./report-template.html.template', import.meta.url), 'utf8');
const line = template.split('\n').find(text => text.startsWith('const PRODUCTS='));
if (!line) throw new Error('PRODUCTS declaration not found in report template');
const declaration = line.split(';')[0] + ';';
for (const data of [{}, { products: ['A', 'B'] }]) {
  const products = new Function('DATA', `${declaration} return PRODUCTS;`)(data);
  if (!Array.isArray(products) || products.length === 0) throw new Error(`no products for ${JSON.stringify(data)}`);
}
console.log('PASS: report template resolves PRODUCTS with and without DATA.products');

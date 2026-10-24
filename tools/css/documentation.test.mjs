import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { extractArticle, normalize, comment } from './documentation.mjs';
test('extracts author purpose and per-value meanings without interactive examples', () => {
  const page = '---\ntitle: box-sizing\n---\n\nThe **`box-sizing`** property controls dimensions.\n\n{{InteractiveExample("demo")}}\n\n## Values\n\n- `border-box`\n  - : Includes {{Cssxref("width")}} and padding.\n\n- `content-box`\n  - : Excludes borders.\n';
  assert.deepEqual(extractArticle(page), {description:'The `box-sizing` property controls dimensions.', values:{'border-box':'Includes `width` and padding.','content-box':'Excludes borders.'}});
});
test('normalizes MDN references and makes comments safe', () => {
  assert.equal(normalize('[CSS](/en-US/docs/Web/CSS) {{cssxref("width", "inline size")}}'), '[CSS](https://developer.mozilla.org/en-US/docs/Web/CSS) `inline size`');
  assert.ok(!comment('unsafe */ text').slice(0,-2).includes('*/'));
});
test('every retained property has a purpose and source in the pinned documentation snapshot', () => {
  const data=JSON.parse(readFileSync(new URL('documentation-data.json', import.meta.url)));
  const bindings=JSON.parse(readFileSync(new URL('../../std/css/bindings.json', import.meta.url)));
  for (const {name} of bindings.properties) {
    assert.ok(data.properties[name]?.description?.length > 30, name);
    assert.match(data.properties[name].source, /^https:\/\//, name);
    assert.doesNotMatch(data.properties[name].description, /\{\{|nominal, specification-constrained/);
  }
});

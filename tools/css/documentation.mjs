// Normalize the pinned MDN prose for generated Terlan comments.
export function normalize(text) {
  return text
    .replace(/\{\{(?:cssxref|htmlelement|svgattr|svgElement|jsxref|domxref|glossary)\(([^{}]*)\)\}\}/gi, (_, args) => {
      const values = [...args.matchAll(/["']([^"']*)["']/g)].map(match => match[1]);
      return '`' + (values[1] || values[0] || '') + '`';
    })
    .replace(/\{\{[^{}]*\}\}/g, '')
    .replace(/\]\(\/en-US\//g, '](https://developer.mozilla.org/en-US/')
    .replace(/<\/?(?:code|em|strong|span|a|abbr|sup|sub)(?:\s[^>]*)?>/g, '')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/&nbsp;/g, ' ').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ').trim();
}

export function extractArticle(markdown) {
  const body = markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  const paragraphs = body.split(/\n\s*\n/);
  const description = normalize(paragraphs.find(p => !/^(?:\s*\{|\s*#|\s*>|\s*```)/.test(p) && p.trim()) || '');
  const values = {};
  // Definition list labels may share one description (for example left/right).
  for (const match of body.matchAll(/^[ \t]*[-*] (.+)\n[ \t]+- : ([^\n]*(?:\n(?!\s*$|[-*] |#{1,6} )[ \t]+[^\n]*)*)/gm)) {
    const meaning = normalize(match[2]);
    for (const label of match[1].matchAll(/`([^`]+)`/g)) {
      if (/^[a-z][a-z0-9-]*$/i.test(label[1]) && meaning) values[label[1]] = meaning;
    }
  }
  return { description, values };
}

export function comment(text) {
  // Keep Markdown paragraphs and avoid terminating a Terlan block comment.
  return '/**\n' + text.replaceAll('*/', '* /').split('\n').flatMap(line => {
    const words = line.split(/\s+/).filter(Boolean);
    if (!words.length) return [' *'];
    const lines = []; let current = ' *';
    for (const word of words) {
      if (current.length > 2 && current.length + word.length + 1 > 100) { lines.push(current); current = ' *'; }
      current += ' ' + word;
    }
    lines.push(current); return lines;
  }).join('\n') + '\n */';
}

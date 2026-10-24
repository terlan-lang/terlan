// Compiler-owned CSS adapter. Project code supplies manifest styles, not a
// Lightning CSS configuration or a second JavaScript build system.
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { bundleAsync, transform } from 'lightningcss';

const externalUrl = /^(?:[a-z][a-z\d+.-]*:|\/\/)/i;

export function rebaseUrl(url, filename) {
  if (/^(?:[a-z][a-z\d+.-]*:|\/|#)/i.test(url)) return url;
  // Query strings and fragments belong to the URL, not the filesystem path.
  const offset = url.search(/[?#]/);
  const file = offset < 0 ? url : url.slice(0, offset);
  const suffix = offset < 0 ? '' : url.slice(offset);
  return path.resolve(path.dirname(filename), file).replaceAll('\\', '/') + suffix;
}

function rebaseImage(image, filename) {
  if (image.type === 'url') {
    return { ...image, value: { ...image.value, url: rebaseUrl(image.value.url, filename) } };
  }
  if (image.type === 'image-set') {
    return {
      ...image,
      value: {
        ...image.value,
        options: image.value.options.map(option => ({
          ...option, image: rebaseImage(option.image, filename),
        })),
      },
    };
  }
  return image;
}

export async function bundleStyles(filename, output) {
  const result = await bundleAsync({
    filename,
    minify: false,
    sourceMap: true,
    resolver: {
      resolve(specifier, originatingFile) {
        return externalUrl.test(specifier)
          ? { external: specifier }
          : path.resolve(path.dirname(originatingFile), specifier);
      },
      async read(filename) {
        const code = await readFile(filename);
        // Rebase before serializing so source maps describe the changed URLs.
        // Rspack then owns copying and fingerprinting these media dependencies.
        const transformed = transform({
          filename,
          code,
          sourceMap: true,
          visitor: {
            Url(value) { return { ...value, url: rebaseUrl(value.url, filename) }; },
            // Lightning CSS visits image-set as one Image, without visiting
            // its nested URLs (including options written as CSS strings).
            Image(value) {
              if (value.type === 'image-set') return rebaseImage(value, filename);
            },
          },
        });
        return transformed.code.toString() + '\n/*# sourceMappingURL=data:application/json;base64,'
          + transformed.map.toString('base64') + ' */';
      },
    },
  });
  const code = result.code.toString() + `\n/*# sourceMappingURL=${path.basename(output)}.map */\n`;
  await writeIfChanged(output + '.map', result.map);
  await writeIfChanged(output, code);
  return code;
}

async function writeIfChanged(filename, content) {
  const bytes = Buffer.from(content);
  if (!(await readFile(filename).catch(() => Buffer.alloc(0))).equals(bytes)) await writeFile(filename, bytes);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 4) throw new Error('usage: css-bundle.mjs <entry.css> <output.css>');
  await bundleStyles(path.resolve(process.argv[2]), path.resolve(process.argv[3]));
}

import http from 'node:http';
import path from 'node:path';
import { readFile } from 'node:fs/promises';
import { output } from './inventory.mjs';

const server = http.createServer(async (request, response) => {
  const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
  if (pathname === '/') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end('<!doctype html><meta charset="utf-8"><title>Terlan browser tests</title><main id="fixture">Terlan</main>');
    return;
  }
  const filename = path.resolve(output, `.${pathname}`);
  if (!filename.startsWith(`${output}${path.sep}`) || !pathname.endsWith('.js')) {
    response.writeHead(404).end();
    return;
  }
  try {
    const bytes = await readFile(filename);
    response.writeHead(200, { 'content-type': 'text/javascript; charset=utf-8', 'cache-control': 'no-store' });
    response.end(bytes);
  } catch {
    response.writeHead(404).end();
  }
});
server.listen(Number(process.env.TERLAN_JS_TEST_PORT ?? 4197), '127.0.0.1');

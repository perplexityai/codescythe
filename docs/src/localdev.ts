#!/usr/bin/env -S node --experimental-transform-types

const { createServer } = require('node:http');
const { createReadStream, existsSync, statSync } = require('node:fs');
const path = require('node:path');
const { build } = require('./render.ts');

type ParsedArgs = {
  host: string;
  port: number;
};

const contentTypes: Record<string, string> = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.txt': 'text/plain; charset=utf-8',
};

function parseArgs(): ParsedArgs {
  let host = process.env.HOST ?? '127.0.0.1';
  let port = Number(process.env.PORT ?? process.env.DOCS_PORT ?? 4173);

  for (let index = 2; index < process.argv.length; index += 1) {
    const arg = process.argv[index];
    if (arg === '--host') {
      host = process.argv[index + 1] ?? host;
      index += 1;
    } else if (arg === '--port') {
      port = Number(process.argv[index + 1] ?? port);
      index += 1;
    }
  }

  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`Invalid docs dev server port: ${port}`);
  }

  return { host, port };
}

function resolveRequestPath(publicDir: string, requestUrl: string | undefined) {
  const url = new URL(requestUrl ?? '/', 'http://localhost');
  let pathname = decodeURIComponent(url.pathname);

  if (pathname === '/codescythe') {
    pathname = '/';
  } else if (pathname.startsWith('/codescythe/')) {
    pathname = pathname.slice('/codescythe'.length);
  }

  const normalizedPath = path.normalize(pathname).replace(/^(\.\.(\/|\\|$))+/, '');
  let filePath = path.join(publicDir, normalizedPath);

  if (!filePath.startsWith(publicDir)) {
    return undefined;
  }

  if (existsSync(filePath) && statSync(filePath).isDirectory()) {
    filePath = path.join(filePath, 'index.html');
  }

  if (!existsSync(filePath) && path.extname(filePath) === '') {
    filePath = path.join(filePath, 'index.html');
  }

  if (!filePath.startsWith(publicDir)) {
    return undefined;
  }

  return filePath;
}

function serve() {
  const { host, port } = parseArgs();
  const { publicDir } = build({ quiet: true });

  const server = createServer((request, response) => {
    const filePath = resolveRequestPath(publicDir, request.url);

    if (!filePath || !existsSync(filePath) || statSync(filePath).isDirectory()) {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end('Not found\n');
      return;
    }

    response.writeHead(200, {
      'cache-control': 'no-store',
      'content-type': contentTypes[path.extname(filePath)] ?? 'application/octet-stream',
    });
    createReadStream(filePath).pipe(response);
  });

  server.listen(port, host, () => {
    console.log(`Codescythe docs: http://${host}:${port}/`);
  });

  process.on('SIGINT', () => {
    server.close(() => process.exit(0));
  });
}

serve();

#!/usr/bin/env -S node --experimental-transform-types

const { spawnSync } = require('node:child_process');

type DeployOptions = {
  dryRun: boolean;
  ref: string;
};

function parseArgs(): DeployOptions {
  let dryRun = false;
  let ref = process.env.DOCS_DEPLOY_REF ?? 'main';

  for (let index = 2; index < process.argv.length; index += 1) {
    const arg = process.argv[index];
    if (arg === '--dry-run') {
      dryRun = true;
    } else if (arg === '--ref') {
      ref = process.argv[index + 1] ?? ref;
      index += 1;
    }
  }

  if (!ref.trim()) {
    throw new Error('Docs deploy ref cannot be empty.');
  }

  return { dryRun, ref };
}

function run(command: string, args: string[], options: { dryRun?: boolean } = {}) {
  const rendered = [command, ...args].join(' ');
  if (options.dryRun) {
    console.log(`[dry-run] ${rendered}`);
    return;
  }

  const result = spawnSync(command, args, {
    stdio: 'inherit',
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${rendered} failed with exit code ${result.status}`);
  }
}

function deploy() {
  const options = parseArgs();

  console.log('Docs are built in CI with: bazel build //docs:build');
  run('gh', ['workflow', 'run', 'pages.yml', '--ref', options.ref], {
    dryRun: options.dryRun,
  });

  if (!options.dryRun) {
    console.log(`Triggered GitHub Pages deployment workflow on ${options.ref}.`);
  }
}

deploy();

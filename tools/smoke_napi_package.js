#!/usr/bin/env node
'use strict';

const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const fs = require('node:fs');
const Module = require('node:module');
const os = require('node:os');
const path = require('node:path');

const [mainPackageArg, nativePackageArg] = process.argv.slice(2);

if (!mainPackageArg || !nativePackageArg) {
  console.error('usage: smoke_napi_package.js <codescythe-package-dir> <native-package-dir>');
  process.exit(2);
}

const repoRoot = path.resolve(__dirname, '..');
const fixture = path.join(repoRoot, 'tests/fixtures/knip-export-basics');
const mainPackageDir = path.resolve(mainPackageArg);
const nativePackageDir = path.resolve(nativePackageArg);
const smokeRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'codescythe-smoke-'));
const nodeModules = path.join(smokeRoot, 'node_modules');

linkPackage(mainPackageDir, nodeModules);
linkPackage(nativePackageDir, nodeModules);

process.env.NODE_PATH = nodeModules;
Module._initPaths();

const native = require(nativePackageDir);
const nativeAnalysis = JSON.parse(native.analyze({cwd: fixture}));
assertFixtureAnalysis(nativeAnalysis);

const codescythe = require('@perplexity/codescythe');
const loaderAnalysis = codescythe.analyze({cwd: fixture});
assertFixtureAnalysis(loaderAnalysis);

const binResult = childProcess.spawnSync(
  process.execPath,
  [path.join(mainPackageDir, 'bin/codescythe.js'), '--json', '-C', fixture],
  {
    encoding: 'utf8',
    env: {
      ...process.env,
      NODE_PATH: nodeModules,
    },
  }
);
assert.equal(binResult.status, 1, binResult.stderr || binResult.stdout);
assertFixtureAnalysis(JSON.parse(binResult.stdout));

console.log(`smoke ok: ${path.basename(nativePackageDir)}`);

function linkPackage(packageDir, nodeModulesDir) {
  const packageJson = require(path.join(packageDir, 'package.json'));
  const parts = packageJson.name.split('/');
  assert.equal(parts.length, 2, `expected scoped package name, got ${packageJson.name}`);

  const scopeDir = path.join(nodeModulesDir, parts[0]);
  fs.mkdirSync(scopeDir, {recursive: true});

  const linkPath = path.join(scopeDir, parts[1]);
  fs.rmSync(linkPath, {force: true, recursive: true});
  fs.symlinkSync(packageDir, linkPath, 'dir');
}

function assertFixtureAnalysis(analysis) {
  assert.ok(analysis.issues.files['dangling.ts']);
  assert.ok(analysis.issues.exports['my-module.ts'].unused);
  assert.ok(analysis.issues.exports['my-module.ts'].default);
  assert.ok(analysis.issues.exports['my-namespace.ts'].key);
  assert.ok(analysis.issues.exports['types.ts'].UnusedType);
  assert.equal(analysis.issues.exports['index.ts'], undefined);
}

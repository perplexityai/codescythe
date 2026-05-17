'use strict';

const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const fs = require('node:fs');
const Module = require('node:module');
const os = require('node:os');
const path = require('node:path');

type Analysis = {
  issues: {
    files: Record<string, unknown>;
    exports: Record<string, Record<string, unknown>>;
  };
};

type NativeBinding = {
  analyze(options: {cwd: string}): string;
};

type Codescythe = {
  analyze(options: {cwd: string}): Analysis;
};

const repoRoot = process.cwd();
const fixture = path.join(repoRoot, 'tests/fixtures/knip-export-basics');
const mainPackageDir = packageDirFromEnv('CODESCYTHE_PACKAGE_DIR');
const nativePackageDir = packageDirFromEnv('CODESCYTHE_NATIVE_PACKAGE_DIR');
const smokeRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'codescythe-smoke-'));
const nodeModules = path.join(smokeRoot, 'node_modules');
let smokeRequire: (specifier: string) => unknown;

describe('@perplexity/codescythe npm package', () => {
  before(() => {
    linkPackage(mainPackageDir, nodeModules);
    linkPackage(nativePackageDir, nodeModules);
    smokeRequire = Module.createRequire(path.join(smokeRoot, 'entry.cjs'));

    process.env.NODE_PATH = nodeModules;
    Module._initPaths();
  });

  it('loads the native package directly', () => {
    const nativePackageJson = require(path.join(nativePackageDir, 'package.json')) as {name: string};
    const native = smokeRequire(nativePackageJson.name) as NativeBinding;
    const analysis = JSON.parse(native.analyze({cwd: fixture})) as Analysis;
    assertFixtureAnalysis(analysis);
  });

  it('loads the platform package through the public package', () => {
    const codescythe = smokeRequire('@perplexity/codescythe') as Codescythe;
    const analysis = codescythe.analyze({cwd: fixture});
    assertFixtureAnalysis(analysis);
  });

  it('runs the public package bin', () => {
    const binResult = childProcess.spawnSync(
      process.execPath,
      [
        '--experimental-transform-types',
        path.join(mainPackageDir, 'bin/codescythe.ts'),
        '--json',
        '-C',
        fixture,
      ],
      {
        encoding: 'utf8',
        env: {
          ...process.env,
          NODE_PATH: nodeModules,
        },
      },
    );

    assert.equal(binResult.status, 1, binResult.stderr || binResult.stdout);
    assertFixtureAnalysis(JSON.parse(binResult.stdout) as Analysis);
  });
});

function packageDirFromEnv(name: string): string {
  const value = process.env[name];
  assert.ok(value, `${name} must point at an unpacked package artifact`);
  return path.resolve(value);
}

function linkPackage(packageDir: string, nodeModulesDir: string): void {
  const packageJson = require(path.join(packageDir, 'package.json')) as {name: string};
  const parts = packageJson.name.split('/');
  assert.equal(parts.length, 2, `expected scoped package name, got ${packageJson.name}`);

  const scopeDir = path.join(nodeModulesDir, parts[0]);
  fs.mkdirSync(scopeDir, {recursive: true});

  const linkPath = path.join(scopeDir, parts[1]);
  fs.rmSync(linkPath, {force: true, recursive: true});
  fs.symlinkSync(packageDir, linkPath, 'dir');
}

function assertFixtureAnalysis(analysis: Analysis): void {
  assert.ok(analysis.issues.files['dangling.ts']);
  assert.ok(analysis.issues.exports['my-module.ts'].unused);
  assert.ok(analysis.issues.exports['my-module.ts'].default);
  assert.ok(analysis.issues.exports['my-namespace.ts'].key);
  assert.ok(analysis.issues.exports['types.ts'].UnusedType);
  assert.equal(analysis.issues.exports['index.ts'], undefined);
}

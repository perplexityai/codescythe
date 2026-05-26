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
    unresolved?: Record<string, string[]>;
  };
  counters: {
    unresolved: number;
  };
  summary?: {
    version: string;
    projectCount: number;
    entryCount: number;
    ignoredUnresolvedCount: number;
    ignoredUnresolvedPatterns: string[];
    packageImportKeys: string[];
    configuredAliasKeys: string[];
  };
};

type FixResult = {
  changedFiles: string[];
  removedFiles: string[];
  removedExports: number;
  analysis: Analysis;
};

type NativeBinding = {
  analyze(options: {config?: string; cwd?: string; fix?: boolean; json?: boolean; verbose?: boolean}): string;
};

type Codescythe = {
  analyze(options: {config?: string; cwd?: string; fix?: boolean; json?: boolean; verbose?: boolean}): Analysis;
  fix(options: {config?: string; cwd?: string; fix?: boolean; json?: boolean; verbose?: boolean}): FixResult;
};

const repoRoot = process.cwd();
const fixture = path.join(repoRoot, 'tests/fixtures/knip-export-basics');
const mainPackageDir = packageDirFromEnv('CODESCYTHE_PACKAGE_DIR');
const nativePackageDir = packageDirFromEnv('CODESCYTHE_NATIVE_PACKAGE_DIR');
const smokeRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'codescythe-smoke-'));
const nodeModules = path.join(smokeRoot, 'node_modules');
let smokeRequire: (specifier: string) => unknown;

describe('codescythe npm package', () => {
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
    const codescythe = smokeRequire('codescythe') as Codescythe;
    const analysis = codescythe.analyze({cwd: fixture});
    assertFixtureAnalysis(analysis);
  });

  it('returns verbose diagnostics through the public package', () => {
    const codescythe = smokeRequire('codescythe') as Codescythe;
    const analysis = codescythe.analyze({cwd: fixture, verbose: true});
    assertFixtureAnalysis(analysis);
    assert.equal(analysis.summary?.entryCount, 1);
    assert.equal(analysis.summary?.projectCount, 5);
    assert.equal(analysis.summary?.ignoredUnresolvedCount, 0);
    assert.deepEqual(analysis.summary?.ignoredUnresolvedPatterns, []);
  });

  it('uses the config parent as the cwd when cwd is omitted', () => {
    const codescythe = smokeRequire('codescythe') as Codescythe;
    const analysis = codescythe.analyze({config: path.join(fixture, 'codescythe.json')});
    assertFixtureAnalysis(analysis);
  });

  it('fixes unused files and exports through the public package', () => {
    const codescythe = smokeRequire('codescythe') as Codescythe;
    const fixFixture = path.join(smokeRoot, 'fix-fixture');
    fs.cpSync(fixture, fixFixture, {recursive: true});

    const result = codescythe.fix({cwd: fixFixture});

    assert.deepEqual(result.removedFiles, ['dangling.ts']);
    assert.deepEqual(result.changedFiles, ['my-module.ts', 'my-namespace.ts', 'types.ts']);
    assert.equal(result.removedExports, 6);
    assertFixtureAnalysis(result.analysis);
    assert.equal(fs.existsSync(path.join(fixFixture, 'dangling.ts')), false);
    const fixedAnalysis = codescythe.analyze({cwd: fixFixture});
    assert.deepEqual(fixedAnalysis.issues.files, {});
    assert.deepEqual(fixedAnalysis.issues.exports, {});
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

  it('runs the public package bin from the config parent', () => {
    const binResult = childProcess.spawnSync(
      process.execPath,
      [
        '--experimental-transform-types',
        path.join(mainPackageDir, 'bin/codescythe.ts'),
        '--json',
        '--config',
        path.join(fixture, 'codescythe.json'),
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

  it('runs the public package bin with verbose diagnostics', () => {
    const binResult = childProcess.spawnSync(
      process.execPath,
      [
        '--experimental-transform-types',
        path.join(mainPackageDir, 'bin/codescythe.ts'),
        '--json',
        '--verbose',
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
    assert.equal(binResult.stderr, '');
    const analysis = JSON.parse(binResult.stdout) as Analysis;
    assertFixtureAnalysis(analysis);
    assert.equal(analysis.summary?.entryCount, 1);
    assert.equal(analysis.summary?.projectCount, 5);
  });
});

function packageDirFromEnv(name: string): string {
  const value = process.env[name];
  assert.ok(value, `${name} must point at an unpacked package artifact`);
  return path.resolve(value);
}

function linkPackage(packageDir: string, nodeModulesDir: string): void {
  const packageJson = require(path.join(packageDir, 'package.json')) as {name: string};
  const parts = packageJson.name.startsWith('@') ? packageJson.name.split('/') : [packageJson.name];
  assert.ok(parts.length === 1 || parts.length === 2, `expected package name, got ${packageJson.name}`);

  const linkParent = parts.length === 2 ? path.join(nodeModulesDir, parts[0]) : nodeModulesDir;
  fs.mkdirSync(linkParent, {recursive: true});

  const linkPath = path.join(linkParent, parts.at(-1)!);
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

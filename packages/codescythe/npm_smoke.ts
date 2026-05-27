import assert from 'node:assert/strict';
import * as childProcess from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import {pathToFileURL} from 'node:url';

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
let smokeBin: string;

describe('codescythe npm package', () => {
  before(() => {
    const mainPackageInstallDir = installPackage(mainPackageDir, nodeModules);
    installPackage(nativePackageDir, nodeModules);
    smokeBin = linkPackageBin(mainPackageInstallDir, nodeModules, 'codescythe');
    fs.writeFileSync(path.join(smokeRoot, 'entry.mjs'), 'export function importPackage(specifier) { return import(specifier); }\n');
  });

  it('publishes JavaScript entrypoints', () => {
    const packageJson = readPackageJson<{
      bin: Record<string, string>;
      files: string[];
      main: string;
      type: string;
    }>(mainPackageDir);
    assert.equal(packageJson.type, 'module');
    assert.equal(packageJson.main, './index.js');
    assert.deepEqual(packageJson.bin, {codescythe: './bin/codescythe.js'});
    assert.equal(packageJson.files.includes('index.ts'), false);
    assert.equal(packageJson.files.includes('index.d.ts'), true);
    assert.equal(packageJson.files.includes('index.js'), true);
    assert.equal(fs.existsSync(path.join(mainPackageDir, 'index.js')), true);
    assert.equal(fs.existsSync(path.join(mainPackageDir, 'index.ts')), false);
    assert.equal(fs.existsSync(path.join(mainPackageDir, 'bin/codescythe.js')), true);
    assert.equal(fs.existsSync(path.join(mainPackageDir, 'bin/codescythe.ts')), false);
    assert.equal(
      fs.readFileSync(path.join(mainPackageDir, 'bin/codescythe.js'), 'utf8').startsWith('#!/usr/bin/env node\n'),
      true,
    );

    const nativePackageJson = readPackageJson<{files: string[]; main: string; type: string}>(nativePackageDir);
    assert.equal(nativePackageJson.type, 'module');
    assert.equal(nativePackageJson.main, './index.js');
    assert.equal(nativePackageJson.files.includes('index.ts'), false);
    assert.equal(nativePackageJson.files.includes('index.js'), true);
    assert.equal(fs.existsSync(path.join(nativePackageDir, 'index.js')), true);
    assert.equal(fs.existsSync(path.join(nativePackageDir, 'index.ts')), false);
  });

  it('loads the native package directly', async () => {
    const nativePackageJson = readPackageJson<{name: string}>(nativePackageDir);
    const native = (await importSmokePackage(nativePackageJson.name)) as NativeBinding;
    const analysis = JSON.parse(native.analyze({cwd: fixture})) as Analysis;
    assertFixtureAnalysis(analysis);
  });

  it('loads the platform package through the public package', async () => {
    const codescythe = (await importSmokePackage('codescythe')) as Codescythe;
    const analysis = codescythe.analyze({cwd: fixture});
    assertFixtureAnalysis(analysis);
  });

  it('returns verbose diagnostics through the public package', async () => {
    const codescythe = (await importSmokePackage('codescythe')) as Codescythe;
    const analysis = codescythe.analyze({cwd: fixture, verbose: true});
    assertFixtureAnalysis(analysis);
    assert.equal(analysis.summary?.entryCount, 1);
    assert.equal(analysis.summary?.projectCount, 5);
    assert.equal(analysis.summary?.ignoredUnresolvedCount, 0);
    assert.deepEqual(analysis.summary?.ignoredUnresolvedPatterns, []);
  });

  it('uses the config parent as the cwd when cwd is omitted', async () => {
    const codescythe = (await importSmokePackage('codescythe')) as Codescythe;
    const analysis = codescythe.analyze({config: path.join(fixture, 'codescythe.json')});
    assertFixtureAnalysis(analysis);
  });

  it('fixes unused files and exports through the public package', async () => {
    const codescythe = (await importSmokePackage('codescythe')) as Codescythe;
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
      smokeBin,
      [
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
      smokeBin,
      [
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
      smokeBin,
      [
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

async function importSmokePackage(specifier: string): Promise<unknown> {
  const entry = (await import(pathToFileURL(path.join(smokeRoot, 'entry.mjs')).href)) as {
    importPackage(specifier: string): Promise<unknown>;
  };
  return entry.importPackage(specifier);
}

function packageDirFromEnv(name: string): string {
  const value = process.env[name];
  assert.ok(value, `${name} must point at an unpacked package artifact`);
  return path.resolve(value);
}

function installPackage(packageDir: string, nodeModulesDir: string): string {
  const packageJson = readPackageJson<{name: string}>(packageDir);
  const installPath = packageInstallPath(packageJson.name, nodeModulesDir);
  fs.rmSync(installPath, {force: true, recursive: true});
  fs.mkdirSync(path.dirname(installPath), {recursive: true});
  fs.cpSync(packageDir, installPath, {recursive: true});
  return installPath;
}

function linkPackageBin(packageDir: string, nodeModulesDir: string, binName: string): string {
  const packageJson = readPackageJson<{bin?: string | Record<string, string>}>(packageDir);
  const relativeBin = typeof packageJson.bin === 'string' ? packageJson.bin : packageJson.bin?.[binName];
  assert.ok(relativeBin, `${binName} bin must be declared`);
  const binPath = path.join(packageDir, relativeBin);
  assert.equal(path.extname(binPath), '.js');
  assert.ok(fs.statSync(binPath).mode & 0o111, `${binPath} must be executable`);

  const binDir = path.join(nodeModulesDir, '.bin');
  fs.mkdirSync(binDir, {recursive: true});
  const linkPath = path.join(binDir, binName);
  fs.rmSync(linkPath, {force: true});
  fs.symlinkSync(binPath, linkPath);
  return linkPath;
}

function readPackageJson<T>(packageDir: string): T {
  return JSON.parse(fs.readFileSync(path.join(packageDir, 'package.json'), 'utf8')) as T;
}

function packageInstallPath(packageName: string, nodeModulesDir: string): string {
  const parts = packageName.split('/');
  if (parts.length === 1) {
    return path.join(nodeModulesDir, packageName);
  }

  assert.equal(parts.length, 2, `expected package name or scoped package name, got ${packageName}`);
  assert.ok(parts[0].startsWith('@'), `expected scoped package name, got ${packageName}`);
  return path.join(nodeModulesDir, parts[0], parts[1]);
}

function assertFixtureAnalysis(analysis: Analysis): void {
  assert.ok(analysis.issues.files['dangling.ts']);
  assert.ok(analysis.issues.exports['my-module.ts'].unused);
  assert.ok(analysis.issues.exports['my-module.ts'].default);
  assert.ok(analysis.issues.exports['my-namespace.ts'].key);
  assert.ok(analysis.issues.exports['types.ts'].UnusedType);
  assert.equal(analysis.issues.exports['index.ts'], undefined);
}

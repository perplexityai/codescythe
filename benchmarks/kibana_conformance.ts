#!/usr/bin/env -S node --experimental-transform-types

const { spawnSync } = require('node:child_process');
const {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');

type Options = {
  codescytheBin?: string;
  knipBin?: string;
  fixturePackageJson?: string;
  fixtureRoot?: string;
  skipBuild: boolean;
  fuzzFiles: number;
  fuzzExports: number;
  seed: number;
  snapshotOutput?: string;
  keepTemp: boolean;
  help: boolean;
};

type JsonRun = {
  value: any;
  elapsedMs: number;
};

type Tool = {
  command: string;
  args: string[];
};

type ImportRef = {
  importer: string;
  specifier: string;
};

type SyntheticExportModule = {
  file: string;
  used: string;
  unused: string[];
};

type FixConformance = {
  codescytheFix: JsonRun;
  postFixCodescythe: JsonRun;
  syntheticFilesMissingFromFixResult: string[];
  syntheticFilesStillOnDisk: string[];
  syntheticExportModulesMissingFromFixResult: string[];
  syntheticExportModulesMissingOnDisk: string[];
  syntheticUnusedExportsStillInSource: string[];
  syntheticUsedExportsMissingFromSource: string[];
  syntheticFilesReportedAfterFix: string[];
  syntheticUnusedExportsReportedAfterFix: string[];
  syntheticUsedExportsReportedAfterFix: string[];
};

type Snapshot = {
  fixture: 'kibana';
  seed: string;
  config: {
    entry: string[];
    project: string[];
    includeEntryExports: boolean;
  };
  fuzz: {
    unusedFiles: string[];
    exportModules: Array<{
      file: string;
      usedExport: string;
      unusedExports: string[];
    }>;
  };
  counters: {
    codescythe: {
      totalFiles: number | 'unknown';
      unusedFiles: number;
      unusedExports: number;
    };
    knip: {
      unusedFiles: number;
    };
  };
  fix: {
    counters: {
      removedFiles: number;
      changedFiles: number;
      removedExports: number;
    };
    postFixCounters: {
      totalFiles: number | 'unknown';
      unusedFiles: number;
      unusedExports: number;
    };
    conformance: {
      syntheticFilesMissingFromFixResult: string[];
      syntheticFilesStillOnDisk: string[];
      syntheticExportModulesMissingFromFixResult: string[];
      syntheticExportModulesMissingOnDisk: string[];
      syntheticUnusedExportsStillInSource: string[];
      syntheticUsedExportsMissingFromSource: string[];
      syntheticFilesReportedAfterFix: string[];
      syntheticUnusedExportsReportedAfterFix: string[];
      syntheticUsedExportsReportedAfterFix: string[];
    };
  };
  conformance: {
    knipUnusedFilesMissingFromCodescythe: string[];
    codescytheOnlyUnusedFiles: string[];
    codescytheOnlyFilesImportedByReachableFiles: Array<{
      file: string;
      importers: ImportRef[];
    }>;
    syntheticFilesMissingFromEitherTool: string[];
    syntheticFilesWithMismatchedReports: string[];
    syntheticUnusedExportsMissingFromCodescythe: string[];
    syntheticUsedExportsReportedByCodescythe: string[];
  };
};

const repoRoot = path.resolve(__dirname, '..');
const defaultCodescytheBin = path.join(
  repoRoot,
  'target',
  'release',
  process.platform === 'win32' ? 'codescythe.exe' : 'codescythe',
);
const benchmarkIgnorePatterns = [
  '**/*.d.ts',
  '**/__fixtures__/**',
  '**/*fixture*/**',
  '**/*fixtures*/**',
  '**/fixtures/**',
  '**/node_modules/**',
  '**/dist/**',
  '**/build/**',
  '**/coverage/**',
  '**/vendor/**',
  '**/.yarn/**',
  '**/.git/**',
  '**/*.gen.ts',
];
const kibanaMarkerTarget = '@benchmark_kibana//:package_json';
const fuzzDirectory = 'codescythe-conformance-fuzz';
const fuzzExportDirectory = `${fuzzDirectory}/exports`;
const kibanaSourceRoots = [
  'src',
  'x-pack',
  'packages',
  'examples',
  'oas_docs',
];
const kibanaEntryPatterns = patternsForRoots(kibanaSourceRoots);
const kibanaProjectRoots = [...kibanaSourceRoots, fuzzDirectory];
const kibanaProjectPatterns = patternsForRoots(kibanaProjectRoots);
const maxJsonBuffer = 1024 * 1024 * 1024;

let executionRoot: string | undefined;
let outputBase: string | undefined;

function patternsForRoots(roots: string[]): string[] {
  return roots.map(root => `${root}/**/*.{ts,tsx,mts,cts}`);
}

const options = parseArgs(process.argv.slice(2));
if (options.help) {
  printHelp();
  process.exit(0);
}

const tempRoot = mkdtempSync(path.join(testTempRoot(), 'codescythe-kibana-conformance-'));

try {
  const sourceFixtureRoot = resolveFixtureRoot(options);
  const fixtureRoot = prepareMutableFixture(sourceFixtureRoot, tempRoot, options);
  const fuzzFiles = writeSyntheticUnusedFiles(fixtureRoot, options.fuzzFiles, options.seed);
  const fuzzExports = writeSyntheticUnusedExports(fixtureRoot, options.fuzzExports, options.seed);
  prepareKibanaFixture(fixtureRoot);

  const codescytheBin = resolveCodescytheBin(options);
  const knipBin = resolveKnipBin(options);
  const pluginNames = readKnipPluginNames(knipBin);
  const codescytheConfig = writeConfig(tempRoot, 'codescythe.json');
  const knipConfig = writeConfig(tempRoot, 'knip.json', pluginNames);

  const codescythe = runJson('codescythe', toolForExecutable(codescytheBin), [
    '--json',
    '--directory',
    fixtureRoot,
    '--config',
    codescytheConfig,
  ], [0, 1]);
  const knip = runJson('knip', toolForExecutable(knipBin), [
    '--no-progress',
    '--no-config-hints',
    '--no-exit-code',
    '--reporter',
    'json',
    '--include',
    'files',
    '--config',
    knipConfig,
  ], [0], fixtureRoot);

  const codescytheUnused = extractCodescytheUnusedFiles(codescythe.value);
  const codescytheUnusedExports = extractCodescytheUnusedExports(codescythe.value);
  const knipUnused = extractKnipUnusedFiles(knip.value, fixtureRoot);
  const missingFromCodescythe = difference(knipUnused, codescytheUnused);
  const extraInCodescythe = difference(codescytheUnused, knipUnused);
  const project = discoverProjectFiles(fixtureRoot);
  const reverseImports = buildReverseImportGraph(
    fixtureRoot,
    project.files,
    project.packageMap,
  );
  const extraReachableImporters = findFilesImportedByReachable(
    extraInCodescythe,
    codescytheUnused,
    reverseImports,
  );
  const missingFuzzFiles = fuzzFiles.filter(file => !codescytheUnused.has(file) || !knipUnused.has(file));
  const unexpectedFuzzFiles = fuzzFiles.filter(file => codescytheUnused.has(file) !== knipUnused.has(file));
  const missingFuzzExports = fuzzExports
    .flatMap(module => module.unused.map(symbol => formatExportRef(module.file, symbol)))
    .filter(ref => !codescytheUnusedExports.has(ref));
  const unexpectedUsedFuzzExports = fuzzExports
    .map(module => formatExportRef(module.file, module.used))
    .filter(ref => codescytheUnusedExports.has(ref));
  const fixConformance = runFixConformance({
    codescytheBin,
    fixtureRoot,
    codescytheConfig,
    fuzzFiles,
    fuzzExports,
  });

  const snapshot = createSnapshot({
    codescythe,
    codescytheUnused,
    codescytheUnusedExports,
    knipUnused,
    missingFromCodescythe,
    extraInCodescythe,
    extraReachableImporters,
    fuzzFiles,
    fuzzExports,
    missingFuzzFiles,
    unexpectedFuzzFiles,
    missingFuzzExports,
    unexpectedUsedFuzzExports,
    fixConformance,
  });

  if (options.snapshotOutput) {
    writeJson(resolveOutputPath(options.snapshotOutput), snapshot);
  }

  printSummary({
    sourceFixtureRoot,
    fixtureRoot,
    tempRoot,
    codescythe,
    knip,
    codescytheUnused,
    codescytheUnusedExports,
    knipUnused,
    missingFromCodescythe,
    extraInCodescythe,
    extraReachableImporters,
    fuzzFiles,
    fuzzExports,
    missingFuzzFiles,
    unexpectedFuzzFiles,
    missingFuzzExports,
    unexpectedUsedFuzzExports,
    fixConformance,
  });

  if (
    missingFromCodescythe.length > 0 ||
    extraReachableImporters.length > 0 ||
    missingFuzzFiles.length > 0 ||
    unexpectedFuzzFiles.length > 0 ||
    missingFuzzExports.length > 0 ||
    unexpectedUsedFuzzExports.length > 0 ||
    fixConformance.syntheticFilesMissingFromFixResult.length > 0 ||
    fixConformance.syntheticFilesStillOnDisk.length > 0 ||
    fixConformance.syntheticExportModulesMissingFromFixResult.length > 0 ||
    fixConformance.syntheticExportModulesMissingOnDisk.length > 0 ||
    fixConformance.syntheticUnusedExportsStillInSource.length > 0 ||
    fixConformance.syntheticUsedExportsMissingFromSource.length > 0 ||
    fixConformance.syntheticFilesReportedAfterFix.length > 0 ||
    fixConformance.syntheticUnusedExportsReportedAfterFix.length > 0 ||
    fixConformance.syntheticUsedExportsReportedAfterFix.length > 0
  ) {
    process.exitCode = 1;
  }
} finally {
  if (!options.keepTemp) {
    rmSync(tempRoot, { recursive: true, force: true });
  }
}

function parseArgs(args: string[]): Options {
  const parsed: Options = {
    codescytheBin: process.env.CODESCYTHE_BIN,
    knipBin: process.env.KNIP_BIN,
    fixturePackageJson: process.env.KIBANA_FIXTURE_PACKAGE_JSON,
    fixtureRoot: process.env.KIBANA_FIXTURE_ROOT,
    skipBuild: false,
    fuzzFiles: 16,
    fuzzExports: 8,
    seed: 0xC0DEC7,
    snapshotOutput: process.env.KIBANA_CONFORMANCE_SNAPSHOT_OUTPUT,
    keepTemp: false,
    help: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--') {
      continue;
    } else if (arg === '--codescythe-bin') {
      parsed.codescytheBin = requireValue(args[++index], arg);
    } else if (arg === '--knip-bin') {
      parsed.knipBin = requireValue(args[++index], arg);
    } else if (arg === '--fixture-package-json') {
      parsed.fixturePackageJson = requireValue(args[++index], arg);
    } else if (arg === '--fixture-root') {
      parsed.fixtureRoot = requireValue(args[++index], arg);
    } else if (arg === '--skip-build') {
      parsed.skipBuild = true;
    } else if (arg === '--fuzz-files') {
      parsed.fuzzFiles = parseNonNegativeInt(args[++index], arg);
    } else if (arg === '--fuzz-exports') {
      parsed.fuzzExports = parseNonNegativeInt(args[++index], arg);
    } else if (arg === '--seed') {
      parsed.seed = parseSeed(args[++index], arg);
    } else if (arg === '--snapshot-output') {
      parsed.snapshotOutput = requireValue(args[++index], arg);
    } else if (arg === '--keep-temp') {
      parsed.keepTemp = true;
    } else if (arg === '--help' || arg === '-h') {
      parsed.help = true;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return parsed;
}

function requireValue(value: string | undefined, flag: string): string {
  if (!value) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function parseNonNegativeInt(value: string | undefined, flag: string): number {
  const parsed = Number.parseInt(requireValue(value, flag), 10);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`${flag} must be a non-negative integer`);
  }
  return parsed;
}

function parseSeed(value: string | undefined, flag: string): number {
  const raw = requireValue(value, flag);
  const parsed = raw.startsWith('0x')
    ? Number.parseInt(raw.slice(2), 16)
    : Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed)) {
    throw new Error(`${flag} must be an integer seed`);
  }
  return parsed >>> 0;
}

function printHelp() {
  console.log(`Usage: node --experimental-transform-types benchmarks/kibana_conformance.ts [options]

Options:
  --codescythe-bin <bin>       Use a specific Codescythe binary
  --knip-bin <bin>             Use a specific Knip binary or bin/knip.js
  --fixture-package-json <bin> Locate the Kibana fixture from its package.json
  --fixture-root <dir>         Use a specific Kibana fixture directory
  --skip-build                 Use target/release/codescythe without rebuilding
  --fuzz-files <n>             Synthetic unused Kibana files to inject (default: 16)
  --fuzz-exports <n>           Synthetic reachable modules with unused exports to inject (default: 8)
  --seed <n>                   Fuzz seed as decimal or 0x-prefixed hex (default: 0xC0DEC7)
  --snapshot-output <file>     Write a stable conformance snapshot JSON file
  --keep-temp                  Keep the copied fixture and generated configs
  -h, --help                   Show this help text
`);
}

function testTempRoot(): string {
  return process.env.TEST_TMPDIR || tmpdir();
}

function resolveCodescytheBin(parsed: Options): string {
  if (parsed.codescytheBin) {
    return resolveExistingPath(parsed.codescytheBin, 'Codescythe binary');
  }

  if (!parsed.skipBuild) {
    const build = spawnSync('cargo', ['build', '--release', '-p', 'codescythe_cli'], {
      cwd: repoRoot,
      encoding: 'utf8',
      stdio: ['ignore', 'inherit', 'pipe'],
    });
    if (build.status !== 0) {
      throw new Error(`cargo build failed:\n${build.stderr}`);
    }
  }

  assertPath(defaultCodescytheBin, 'Codescythe release binary');
  return defaultCodescytheBin;
}

function resolveKnipBin(parsed: Options): string {
  if (parsed.knipBin) {
    return resolveExistingPath(parsed.knipBin, 'Knip binary');
  }

  const localBin = path.join(
    repoRoot,
    'node_modules',
    '.bin',
    process.platform === 'win32' ? 'knip.cmd' : 'knip',
  );
  const packageBin = path.join(repoRoot, 'node_modules', 'knip', 'bin', 'knip.js');
  if (existsSync(packageBin)) {
    return packageBin;
  }
  if (canRun(toolForExecutable(localBin), ['--version'])) {
    return localBin;
  }
  if (canRun({ command: 'knip', args: [] }, ['--version'])) {
    return 'knip';
  }
  throw new Error('Knip binary not found; run pnpm install or pass --knip-bin');
}

function assertPath(command: string, label: string) {
  if (!existsSync(command)) {
    throw new Error(`${label} not found at ${command}`);
  }
}

function resolveExistingPath(filePath: string, label: string): string {
  const candidates = pathCandidates(filePath);
  const found = candidates.find(existsSync);
  if (found) {
    return found;
  }
  throw new Error(`${label} not found; tried:\n${candidates.map(candidate => `  ${candidate}`).join('\n')}`);
}

function pathCandidates(filePath: string): string[] {
  const candidates: string[] = [];
  const add = (candidate: string) => {
    if (!candidates.includes(candidate)) {
      candidates.push(candidate);
    }
  };
  add(path.resolve(filePath));

  if (!path.isAbsolute(filePath)) {
    for (const execRoot of [
      process.env.JS_BINARY__EXECROOT,
      process.env.BAZEL_EXECROOT,
      execRootFromCwd(),
    ]) {
      if (execRoot) {
        add(path.join(execRoot, filePath));
      }
    }
  }

  const logicalPaths = runfileLogicalPaths(filePath);
  for (const runfilesRoot of [
    process.env.JS_BINARY__RUNFILES,
    process.env.RUNFILES_DIR,
    process.env.TEST_SRCDIR,
  ]) {
    if (!runfilesRoot) {
      continue;
    }
    for (const logical of logicalPaths) {
      add(path.join(runfilesRoot, logical));
      add(path.join(runfilesRoot, process.env.TEST_WORKSPACE || '_main', logical));
    }
  }

  return candidates;
}

function execRootFromCwd(): string | undefined {
  const cwd = normalizeFixturePath(process.cwd());
  const marker = '/bazel-out/';
  const index = cwd.indexOf(marker);
  if (index < 0) {
    return undefined;
  }
  return cwd.slice(0, index);
}

function resolveOutputPath(filePath: string): string {
  if (path.isAbsolute(filePath)) {
    return filePath;
  }
  if (filePath.startsWith('bazel-out/')) {
    const execRoot = execRootFromCwd();
    if (execRoot) {
      return path.join(execRoot, filePath);
    }
  }
  return path.resolve(filePath);
}

function runfileLogicalPaths(filePath: string): string[] {
  const normalized = normalizeFixturePath(filePath);
  const logicals: string[] = [normalized.replace(/^\/+/, '')];
  for (const marker of ['/external/', 'external/', '/execroot/_main/']) {
    const index = normalized.lastIndexOf(marker);
    if (index >= 0) {
      logicals.push(normalized.slice(index + marker.length));
    }
  }
  if (normalized.startsWith('_main/')) {
    logicals.push(normalized.slice('_main/'.length));
  }
  return [...new Set(logicals)];
}

function toolForExecutable(command: string): Tool {
  return command.endsWith('.js')
    ? { command: process.execPath, args: [command] }
    : { command, args: [] };
}

function canRun(tool: Tool, args: string[]) {
  const result = spawnSync(tool.command, [...tool.args, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: 'ignore',
  });
  return result.status === 0;
}

function resolveFixtureRoot(parsed: Options): string {
  if (parsed.fixtureRoot) {
    const packageJson = resolveExistingPath(
      path.join(parsed.fixtureRoot, 'package.json'),
      'Kibana fixture package.json',
    );
    return realpathSync(path.dirname(packageJson));
  }
  if (parsed.fixturePackageJson) {
    const packageJson = resolveExistingPath(parsed.fixturePackageJson, 'Kibana fixture package.json');
    return realpathSync(path.dirname(packageJson));
  }

  const markerPath = bazelStdout([
    'cquery',
    kibanaMarkerTarget,
    '--output=files',
    '--noshow_progress',
  ])
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(Boolean)
    .find(line => line.endsWith('package.json'));

  if (!markerPath) {
    throw new Error(`Bazel did not return package.json for ${kibanaMarkerTarget}`);
  }

  const absoluteMarkerPath = path.isAbsolute(markerPath)
    ? markerPath
    : markerPath.startsWith('external/')
      ? path.resolve(getOutputBase(), markerPath)
      : path.resolve(getExecutionRoot(), markerPath);
  if (!existsSync(absoluteMarkerPath)) {
    throw new Error(`Bazel fixture marker does not exist: ${absoluteMarkerPath}`);
  }
  return realpathSync(path.dirname(absoluteMarkerPath));
}

function getExecutionRoot(): string {
  if (!executionRoot) {
    executionRoot = bazelStdout(['info', 'execution_root', '--noshow_progress']);
  }
  return executionRoot;
}

function getOutputBase(): string {
  if (!outputBase) {
    outputBase = bazelStdout(['info', 'output_base', '--noshow_progress']);
  }
  return outputBase;
}

function bazelStdout(args: string[]): string {
  const result = spawnSync('bazel', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status !== 0) {
    throw new Error(
      `bazel ${args.join(' ')} failed with exit code ${result.status ?? 'unknown'}:\n` +
        result.stderr,
    );
  }
  return result.stdout.trim();
}

function prepareMutableFixture(sourceFixtureRoot: string, directory: string, parsed: Options): string {
  const fixtureRoot = path.join(directory, 'kibana');
  cpSync(sourceFixtureRoot, fixtureRoot, {
    recursive: true,
    dereference: true,
    filter: source => {
      const relative = normalizeFixturePath(path.relative(sourceFixtureRoot, source));
      return relative !== '.git' && relative !== 'node_modules';
    },
  });
  sanitizeRootManifest(fixtureRoot);
  if (parsed.keepTemp) {
    console.log(`Copied fixture: ${fixtureRoot}`);
  }
  return fixtureRoot;
}

function sanitizeRootManifest(fixtureRoot: string) {
  const packageJsonPath = path.join(fixtureRoot, 'package.json');
  const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  for (const field of [
    'bin',
    'browser',
    'dependencies',
    'devDependencies',
    'exports',
    'imports',
    'main',
    'module',
    'optionalDependencies',
    'peerDependencies',
    'scripts',
    'types',
    'typings',
    'workspaces',
  ]) {
    delete packageJson[field];
  }
  writeJson(packageJsonPath, packageJson);
}

function writeSyntheticUnusedFiles(fixtureRoot: string, count: number, seed: number): string[] {
  if (count === 0) {
    return [];
  }
  const directory = path.join(fixtureRoot, fuzzDirectory);
  mkdirSync(directory, { recursive: true });
  const files: string[] = [];
  let state = seed >>> 0;
  for (let index = 0; index < count; index += 1) {
    state = (state * 1664525 + 1013904223) >>> 0;
    const relative = `${fuzzDirectory}/unused_${index}_${state.toString(16)}.ts`;
    writeFileSync(
      path.join(fixtureRoot, relative),
      `export const unused${index} = ${state};\n`,
    );
    files.push(relative);
  }
  return files.sort();
}

function writeSyntheticUnusedExports(
  fixtureRoot: string,
  count: number,
  seed: number,
): SyntheticExportModule[] {
  if (count === 0) {
    return [];
  }

  const importer = 'src/core/server/index.ts';
  const importerPath = path.join(fixtureRoot, importer);
  assertPath(importerPath, 'Kibana server entry');
  mkdirSync(path.join(fixtureRoot, fuzzExportDirectory), { recursive: true });

  let state = (seed ^ 0x9E3779B9) >>> 0;
  const importLines: string[] = [];
  const useLines: string[] = [];
  const modules: SyntheticExportModule[] = [];

  for (let index = 0; index < count; index += 1) {
    state = (state * 1664525 + 1013904223) >>> 0;
    const suffix = `${index}_${state.toString(16)}`;
    const baseName = `codescythe_conformance_export_${suffix}`;
    const file = `${fuzzExportDirectory}/${baseName}.ts`;
    const used = `usedFuzzExport_${suffix}`;
    const unused = [
      `unusedValueFuzzExport_${suffix}`,
      `UnusedTypeFuzzExport_${suffix}`,
    ];

    writeFileSync(
      path.join(fixtureRoot, file),
      [
        `export const ${used} = ${state};`,
        `export const ${unused[0]} = ${state + 1};`,
        `export type ${unused[1]} = { value: number };`,
        '',
      ].join('\n'),
    );
    importLines.push(`import { ${used} } from '../../../${fuzzExportDirectory}/${baseName}';`);
    useLines.push(`void ${used};`);
    modules.push({ file, used, unused });
  }

  const original = readFileSync(importerPath, 'utf8');
  writeFileSync(
    importerPath,
    [
      ...importLines,
      ...useLines,
      '',
      original,
    ].join('\n'),
  );
  return modules;
}

function prepareKibanaFixture(fixtureRoot: string) {
  const tsconfigBaseDir = path.join(
    fixtureRoot,
    'node_modules',
    '@kbn',
    'tsconfig-base',
  );
  mkdirSync(tsconfigBaseDir, { recursive: true });
  writeJson(path.join(tsconfigBaseDir, 'tsconfig.json'), {
    extends: '../../../tsconfig.base.json',
  });
}

function writeConfig(
  directory: string,
  fileName: string,
  disabledKnipPlugins: string[] = [],
): string {
  const configPath = path.join(directory, fileName);
  const config: Record<string, unknown> = {
    entry: kibanaEntryPatterns,
    project: kibanaProjectPatterns,
    ignore: benchmarkIgnorePatterns,
    includeEntryExports: false,
    ignoreExportsUsedInFile: false,
  };
  for (const pluginName of disabledKnipPlugins) {
    config[pluginName] = false;
  }
  writeJson(configPath, config);
  return configPath;
}

function writeJson(filePath: string, value: unknown) {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function readKnipPluginNames(knipBin: string): string[] {
  const candidates = [
    path.join(path.dirname(path.dirname(knipBin)), 'dist', 'types', 'PluginNames.js'),
    path.join(repoRoot, 'node_modules', 'knip', 'dist', 'types', 'PluginNames.js'),
  ];
  const pluginNamesPath = candidates.find(existsSync);
  if (!pluginNamesPath) {
    throw new Error('Unable to locate Knip PluginNames.js to disable plugins');
  }
  const source = readFileSync(pluginNamesPath, 'utf8');
  return [...source.matchAll(/'([^']+)'/g)].map(match => match[1]);
}

function runJson(
  label: string,
  tool: Tool,
  args: string[],
  okStatuses: number[],
  cwd = repoRoot,
): JsonRun {
  const started = Date.now();
  const result = spawnSync(tool.command, [...tool.args, ...args], {
    cwd,
    encoding: 'utf8',
    maxBuffer: maxJsonBuffer,
    env: {
      ...process.env,
      CI: '1',
      NO_COLOR: '1',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (!okStatuses.includes(result.status ?? -1)) {
    throw new Error(
      `${label} failed with exit code ${result.status ?? 'unknown'}:\n${result.stderr}`,
    );
  }
  try {
    return {
      value: JSON.parse(result.stdout),
      elapsedMs: Date.now() - started,
    };
  } catch (error) {
    throw new Error(`${label} did not print JSON:\n${String(error)}\n${result.stdout}`);
  }
}

function runFixConformance(options: {
  codescytheBin: string;
  fixtureRoot: string;
  codescytheConfig: string;
  fuzzFiles: string[];
  fuzzExports: SyntheticExportModule[];
}): FixConformance {
  const codescytheTool = toolForExecutable(options.codescytheBin);
  const codescytheFix = runJson('codescythe --fix', codescytheTool, [
    '--fix',
    '--json',
    '--directory',
    options.fixtureRoot,
    '--config',
    options.codescytheConfig,
  ], [0, 1]);
  const removedFiles = new Set(
    (codescytheFix.value?.removedFiles ?? []).map((file: string) => normalizeFixturePath(file)),
  );
  const changedFiles = new Set(
    (codescytheFix.value?.changedFiles ?? []).map((file: string) => normalizeFixturePath(file)),
  );

  const syntheticFilesMissingFromFixResult = options.fuzzFiles
    .filter(file => !removedFiles.has(file));
  const syntheticFilesStillOnDisk = options.fuzzFiles
    .filter(file => existsSync(path.join(options.fixtureRoot, file)));
  const syntheticExportModulesMissingFromFixResult = options.fuzzExports
    .map(module => module.file)
    .filter(file => !changedFiles.has(file));
  const sourceChecks = inspectFixedSyntheticExports(options.fixtureRoot, options.fuzzExports);

  const postFixCodescythe = runJson('codescythe after --fix', codescytheTool, [
    '--json',
    '--directory',
    options.fixtureRoot,
    '--config',
    options.codescytheConfig,
  ], [0, 1]);
  const postFixUnusedFiles = extractCodescytheUnusedFiles(postFixCodescythe.value);
  const postFixUnusedExports = extractCodescytheUnusedExports(postFixCodescythe.value);

  return {
    codescytheFix,
    postFixCodescythe,
    syntheticFilesMissingFromFixResult,
    syntheticFilesStillOnDisk,
    syntheticExportModulesMissingFromFixResult,
    syntheticExportModulesMissingOnDisk: sourceChecks.syntheticExportModulesMissingOnDisk,
    syntheticUnusedExportsStillInSource: sourceChecks.syntheticUnusedExportsStillInSource,
    syntheticUsedExportsMissingFromSource: sourceChecks.syntheticUsedExportsMissingFromSource,
    syntheticFilesReportedAfterFix: options.fuzzFiles
      .filter(file => postFixUnusedFiles.has(file)),
    syntheticUnusedExportsReportedAfterFix: options.fuzzExports
      .flatMap(module => module.unused.map(symbol => formatExportRef(module.file, symbol)))
      .filter(ref => postFixUnusedExports.has(ref)),
    syntheticUsedExportsReportedAfterFix: options.fuzzExports
      .map(module => formatExportRef(module.file, module.used))
      .filter(ref => postFixUnusedExports.has(ref)),
  };
}

function inspectFixedSyntheticExports(
  fixtureRoot: string,
  modules: SyntheticExportModule[],
): {
  syntheticExportModulesMissingOnDisk: string[];
  syntheticUnusedExportsStillInSource: string[];
  syntheticUsedExportsMissingFromSource: string[];
} {
  const syntheticExportModulesMissingOnDisk: string[] = [];
  const syntheticUnusedExportsStillInSource: string[] = [];
  const syntheticUsedExportsMissingFromSource: string[] = [];

  for (const module of modules) {
    const filePath = path.join(fixtureRoot, module.file);
    if (!existsSync(filePath)) {
      syntheticExportModulesMissingOnDisk.push(module.file);
      continue;
    }

    const source = readFileSync(filePath, 'utf8');
    for (const symbol of module.unused) {
      if (source.includes(symbol)) {
        syntheticUnusedExportsStillInSource.push(formatExportRef(module.file, symbol));
      }
    }
    if (!source.includes(module.used)) {
      syntheticUsedExportsMissingFromSource.push(formatExportRef(module.file, module.used));
    }
  }

  return {
    syntheticExportModulesMissingOnDisk,
    syntheticUnusedExportsStillInSource,
    syntheticUsedExportsMissingFromSource,
  };
}

function extractCodescytheUnusedFiles(analysis: any): Set<string> {
  return new Set(Object.keys(analysis?.issues?.files ?? {}).map(file => normalizeFixturePath(file)));
}

function extractCodescytheUnusedExports(analysis: any): Set<string> {
  const exports = new Set<string>();
  for (const [file, symbols] of Object.entries(analysis?.issues?.exports ?? {})) {
    for (const symbol of Object.keys(symbols as Record<string, unknown>)) {
      exports.add(formatExportRef(normalizeFixturePath(file), symbol));
    }
  }
  return exports;
}

function extractKnipUnusedFiles(report: any, fixtureRoot: string): Set<string> {
  const files = new Set<string>();
  for (const issue of report?.issues ?? []) {
    if (issue.file && Array.isArray(issue.files) && issue.files.length > 0) {
      files.add(normalizeFixturePath(issue.file, fixtureRoot));
    }
    for (const file of issue.files ?? []) {
      const name = typeof file === 'string' ? file : file?.name ?? file?.file ?? file?.path;
      if (name) {
        files.add(normalizeFixturePath(name, fixtureRoot));
      }
    }
  }
  return files;
}

function normalizeFixturePath(filePath: string, fixtureRoot?: string): string {
  const relative = fixtureRoot && path.isAbsolute(filePath)
    ? path.relative(fixtureRoot, filePath)
    : filePath;
  return relative.split(path.sep).join('/').replace(/^\.\//, '');
}

function formatExportRef(file: string, symbol: string): string {
  return `${normalizeFixturePath(file)}#${symbol}`;
}

function difference(left: Set<string>, right: Set<string>): string[] {
  return [...left].filter(value => !right.has(value)).sort();
}

function discoverProjectFiles(fixtureRoot: string): {
  files: string[];
  packageMap: Map<string, string>;
} {
  const files: string[] = [];
  const packageMap = new Map<string, string>();

  walkFixture(fixtureRoot, '', {
    file(filePath, relative) {
      if (path.basename(relative) === 'package.json') {
        try {
          const packageJson = JSON.parse(readFileSync(filePath, 'utf8'));
          if (typeof packageJson.name === 'string' && packageJson.name.startsWith('@kbn/')) {
            packageMap.set(packageJson.name, path.dirname(relative));
          }
        } catch {}
      }

      if (
        isProjectSourceFile(relative) &&
        isConfiguredProjectPath(relative) &&
        !isIgnoredPath(relative)
      ) {
        files.push(relative);
      }
    },
  });

  files.sort();
  return { files, packageMap };
}

function walkFixture(
  root: string,
  relativeDirectory: string,
  visitor: { file(filePath: string, relative: string): void },
) {
  const directory = path.join(root, relativeDirectory);
  for (const dirent of readdirSync(directory, { withFileTypes: true })) {
    const relative = normalizeFixturePath(path.join(relativeDirectory, dirent.name));
    if (dirent.isDirectory()) {
      if (!isIgnoredDirectory(dirent.name)) {
        walkFixture(root, relative, visitor);
      }
    } else if (dirent.isFile()) {
      visitor.file(path.join(root, relative), relative);
    }
  }
}

function isProjectSourceFile(relative: string): boolean {
  return /\.(ts|tsx|mts|cts)$/.test(relative) && !relative.endsWith('.d.ts');
}

function isConfiguredProjectPath(relative: string): boolean {
  return kibanaProjectRoots.some(root => relative === root || relative.startsWith(`${root}/`));
}

function isIgnoredDirectory(name: string): boolean {
  return (
    name === '.git' ||
    name === '.yarn' ||
    name === 'bazel-bin' ||
    name === 'bazel-out' ||
    name === 'bazel-testlogs' ||
    name === 'build' ||
    name === 'coverage' ||
    name === 'dist' ||
    name === 'node_modules' ||
    name === 'target' ||
    name === 'vendor' ||
    name === '__fixtures__' ||
    name.includes('fixture')
  );
}

function isIgnoredPath(relative: string): boolean {
  return relative.endsWith('.gen.ts') || relative.split('/').some(isIgnoredDirectory);
}

function buildReverseImportGraph(
  fixtureRoot: string,
  projectFiles: string[],
  packageMap: Map<string, string>,
): Map<string, ImportRef[]> {
  const projectSet = new Set(projectFiles);
  const reverseImports = new Map<string, ImportRef[]>();

  for (const importer of projectFiles) {
    const source = readFileSync(path.join(fixtureRoot, importer), 'utf8');
    for (const specifier of extractImportSpecifiers(source)) {
      const target = resolveImport(importer, specifier, projectSet, packageMap);
      if (!target) {
        continue;
      }
      const refs = reverseImports.get(target) ?? [];
      refs.push({ importer, specifier });
      reverseImports.set(target, refs);
    }
  }

  return reverseImports;
}

function extractImportSpecifiers(source: string): string[] {
  const specifiers: string[] = [];
  const staticImport =
    /\b(?:import|export)\s+(?:type\s+)?(?:[\s\S]{0,220}?\s+from\s*)?["']([^"']+)["']/g;
  const dynamicImport = /\bimport\s*\(\s*["']([^"']+)["']\s*\)/g;

  let match: RegExpExecArray | null;
  while ((match = staticImport.exec(source))) {
    specifiers.push(match[1]);
  }
  while ((match = dynamicImport.exec(source))) {
    specifiers.push(match[1]);
  }
  return specifiers;
}

function resolveImport(
  importer: string,
  specifier: string,
  projectSet: Set<string>,
  packageMap: Map<string, string>,
): string | undefined {
  if (specifier.startsWith('.')) {
    return resolveFile(
      normalizeFixturePath(path.join(path.dirname(importer), specifier)),
      projectSet,
    );
  }

  if (specifier.startsWith('@kbn/')) {
    const parts = specifier.split('/');
    const packageName = parts.slice(0, 2).join('/');
    const packageDirectory = packageMap.get(packageName);
    if (!packageDirectory) {
      return undefined;
    }
    const subpath = parts.slice(2).join('/');
    return resolveFile(normalizeFixturePath(path.join(packageDirectory, subpath || 'index')), projectSet);
  }

  return undefined;
}

function resolveFile(candidate: string, projectSet: Set<string>): string | undefined {
  const normalized = candidate.replace(/^\.\//, '');
  const extension = path.extname(normalized);
  if (extension) {
    for (const file of extensionAliasCandidates(normalized, extension)) {
      if (projectSet.has(file)) {
        return file;
      }
    }
    return undefined;
  }

  for (const extension of ['.ts', '.tsx', '.mts', '.cts']) {
    const file = `${normalized}${extension}`;
    if (projectSet.has(file)) {
      return file;
    }
  }
  for (const extension of ['.ts', '.tsx', '.mts', '.cts']) {
    const file = `${normalized}/index${extension}`;
    if (projectSet.has(file)) {
      return file;
    }
  }
  return undefined;
}

function extensionAliasCandidates(filePath: string, extension: string): string[] {
  const withoutExtension = filePath.slice(0, -extension.length);
  const aliases = {
    '.js': ['.ts', '.tsx', '.js', '.jsx'],
    '.jsx': ['.tsx', '.jsx'],
    '.mjs': ['.mts', '.mjs'],
    '.cjs': ['.cts', '.cjs'],
  }[extension] ?? [extension];
  return aliases.map(alias => `${withoutExtension}${alias}`);
}

function findFilesImportedByReachable(
  files: string[],
  unusedFiles: Set<string>,
  reverseImports: Map<string, ImportRef[]>,
): Array<{ file: string; importers: ImportRef[] }> {
  const failures: Array<{ file: string; importers: ImportRef[] }> = [];
  for (const file of files) {
    const reachableImporters = (reverseImports.get(file) ?? [])
      .filter(ref => !unusedFiles.has(ref.importer));
    if (reachableImporters.length > 0) {
      failures.push({
        file,
        importers: reachableImporters,
      });
    }
  }
  failures.sort((left, right) => left.file.localeCompare(right.file));
  return failures;
}

function createSnapshot(summary: {
  codescythe: JsonRun;
  codescytheUnused: Set<string>;
  codescytheUnusedExports: Set<string>;
  knipUnused: Set<string>;
  missingFromCodescythe: string[];
  extraInCodescythe: string[];
  extraReachableImporters: Array<{ file: string; importers: ImportRef[] }>;
  fuzzFiles: string[];
  fuzzExports: SyntheticExportModule[];
  missingFuzzFiles: string[];
  unexpectedFuzzFiles: string[];
  missingFuzzExports: string[];
  unexpectedUsedFuzzExports: string[];
  fixConformance: FixConformance;
}): Snapshot {
  const analysisCounters = summary.codescythe.value.counters ?? {};
  const fixCounters = summary.fixConformance.codescytheFix.value ?? {};
  const postFixCounters = summary.fixConformance.postFixCodescythe.value.counters ?? {};
  return {
    fixture: 'kibana',
    seed: `0x${options.seed.toString(16)}`,
    config: {
      entry: kibanaEntryPatterns,
      project: kibanaProjectPatterns,
      includeEntryExports: false,
    },
    fuzz: {
      unusedFiles: summary.fuzzFiles,
      exportModules: summary.fuzzExports.map(module => ({
        file: module.file,
        usedExport: module.used,
        unusedExports: module.unused,
      })),
    },
    counters: {
      codescythe: {
        totalFiles: analysisCounters.total ?? 'unknown',
        unusedFiles: summary.codescytheUnused.size,
        unusedExports: summary.codescytheUnusedExports.size,
      },
      knip: {
        unusedFiles: summary.knipUnused.size,
      },
    },
    fix: {
      counters: {
        removedFiles: fixCounters.removedFiles?.length ?? 0,
        changedFiles: fixCounters.changedFiles?.length ?? 0,
        removedExports: fixCounters.removedExports ?? 0,
      },
      postFixCounters: {
        totalFiles: postFixCounters.total ?? 'unknown',
        unusedFiles: postFixCounters.files ?? 0,
        unusedExports: postFixCounters.exports ?? 0,
      },
      conformance: {
        syntheticFilesMissingFromFixResult: summary.fixConformance.syntheticFilesMissingFromFixResult,
        syntheticFilesStillOnDisk: summary.fixConformance.syntheticFilesStillOnDisk,
        syntheticExportModulesMissingFromFixResult: summary.fixConformance.syntheticExportModulesMissingFromFixResult,
        syntheticExportModulesMissingOnDisk: summary.fixConformance.syntheticExportModulesMissingOnDisk,
        syntheticUnusedExportsStillInSource: summary.fixConformance.syntheticUnusedExportsStillInSource,
        syntheticUsedExportsMissingFromSource: summary.fixConformance.syntheticUsedExportsMissingFromSource,
        syntheticFilesReportedAfterFix: summary.fixConformance.syntheticFilesReportedAfterFix,
        syntheticUnusedExportsReportedAfterFix: summary.fixConformance.syntheticUnusedExportsReportedAfterFix,
        syntheticUsedExportsReportedAfterFix: summary.fixConformance.syntheticUsedExportsReportedAfterFix,
      },
    },
    conformance: {
      knipUnusedFilesMissingFromCodescythe: summary.missingFromCodescythe,
      codescytheOnlyUnusedFiles: summary.extraInCodescythe,
      codescytheOnlyFilesImportedByReachableFiles: summary.extraReachableImporters.map(failure => ({
        file: failure.file,
        importers: failure.importers
          .map(ref => ({
            importer: ref.importer,
            specifier: ref.specifier,
          }))
          .sort(compareImportRef),
      })),
      syntheticFilesMissingFromEitherTool: summary.missingFuzzFiles,
      syntheticFilesWithMismatchedReports: summary.unexpectedFuzzFiles,
      syntheticUnusedExportsMissingFromCodescythe: summary.missingFuzzExports,
      syntheticUsedExportsReportedByCodescythe: summary.unexpectedUsedFuzzExports,
    },
  };
}

function compareImportRef(left: ImportRef, right: ImportRef): number {
  return left.importer.localeCompare(right.importer) ||
    left.specifier.localeCompare(right.specifier);
}

function printSummary(summary: {
  sourceFixtureRoot: string;
  fixtureRoot: string;
  tempRoot: string;
  codescythe: JsonRun;
  knip: JsonRun;
  codescytheUnused: Set<string>;
  codescytheUnusedExports: Set<string>;
  knipUnused: Set<string>;
  missingFromCodescythe: string[];
  extraInCodescythe: string[];
  extraReachableImporters: Array<{ file: string; importers: ImportRef[] }>;
  fuzzFiles: string[];
  fuzzExports: SyntheticExportModule[];
  missingFuzzFiles: string[];
  unexpectedFuzzFiles: string[];
  missingFuzzExports: string[];
  unexpectedUsedFuzzExports: string[];
  fixConformance: FixConformance;
}) {
  const analysisCounters = summary.codescythe.value.counters ?? {};
  const fixCounters = summary.fixConformance.codescytheFix.value ?? {};
  const postFixCounters = summary.fixConformance.postFixCodescythe.value.counters ?? {};

  console.log(`Fixture: Kibana`);
  console.log(`Source root: ${summary.sourceFixtureRoot}`);
  console.log(`Working root: ${summary.fixtureRoot}`);
  console.log(
    `Runs: codescythe ${formatMs(summary.codescythe.elapsedMs)}, ` +
    `codescythe --fix ${formatMs(summary.fixConformance.codescytheFix.elapsedMs)}, ` +
    `post-fix codescythe ${formatMs(summary.fixConformance.postFixCodescythe.elapsedMs)}, ` +
    `knip ${formatMs(summary.knip.elapsedMs)}`,
  );
  console.log(`Codescythe files: ${summary.codescytheUnused.size} unused / ${analysisCounters.total ?? 'unknown'} total`);
  console.log(`Codescythe exports: ${summary.codescytheUnusedExports.size} unused`);
  console.log(`Knip files: ${summary.knipUnused.size} unused`);
  console.log(`Config entry: ${formatPatterns(kibanaEntryPatterns)}`);
  console.log(`Config project: ${formatPatterns(kibanaProjectPatterns)}`);
  console.log(
    `Fix: ${fixCounters.removedFiles?.length ?? 0} files removed, ` +
    `${fixCounters.removedExports ?? 0} exports removed from ` +
    `${fixCounters.changedFiles?.length ?? 0} files`,
  );
  console.log(`Post-fix Codescythe files: ${postFixCounters.files ?? 0} unused / ${postFixCounters.total ?? 'unknown'} total`);
  console.log(`Post-fix Codescythe exports: ${postFixCounters.exports ?? 0} unused`);
  console.log(`Synthetic unused files: ${summary.fuzzFiles.length}`);
  console.log(`Synthetic export modules: ${summary.fuzzExports.length}`);
  console.log('');
  console.log(`Conformance:`);
  console.log(`  Knip unused files missing from Codescythe: ${summary.missingFromCodescythe.length}`);
  console.log(`  Codescythe-only unused files: ${summary.extraInCodescythe.length}`);
  console.log(`  Codescythe-only files imported by reachable files: ${summary.extraReachableImporters.length}`);
  console.log(`  Synthetic files missing from either tool: ${summary.missingFuzzFiles.length}`);
  console.log(`  Synthetic files with mismatched reports: ${summary.unexpectedFuzzFiles.length}`);
  console.log(`  Synthetic unused exports missing from Codescythe: ${summary.missingFuzzExports.length}`);
  console.log(`  Synthetic used exports reported by Codescythe: ${summary.unexpectedUsedFuzzExports.length}`);
  console.log(`  Synthetic files missing from fix result: ${summary.fixConformance.syntheticFilesMissingFromFixResult.length}`);
  console.log(`  Synthetic files still on disk after fix: ${summary.fixConformance.syntheticFilesStillOnDisk.length}`);
  console.log(`  Synthetic export modules missing from fix result: ${summary.fixConformance.syntheticExportModulesMissingFromFixResult.length}`);
  console.log(`  Synthetic export modules missing on disk after fix: ${summary.fixConformance.syntheticExportModulesMissingOnDisk.length}`);
  console.log(`  Synthetic unused exports still in source after fix: ${summary.fixConformance.syntheticUnusedExportsStillInSource.length}`);
  console.log(`  Synthetic used exports missing from source after fix: ${summary.fixConformance.syntheticUsedExportsMissingFromSource.length}`);
  console.log(`  Synthetic files reported after fix: ${summary.fixConformance.syntheticFilesReportedAfterFix.length}`);
  console.log(`  Synthetic unused exports reported after fix: ${summary.fixConformance.syntheticUnusedExportsReportedAfterFix.length}`);
  console.log(`  Synthetic used exports reported after fix: ${summary.fixConformance.syntheticUsedExportsReportedAfterFix.length}`);

  printExamples('Missing from Codescythe', summary.missingFromCodescythe);
  printExamples('Codescythe-only unused files', summary.extraInCodescythe);
  if (summary.extraReachableImporters.length > 0) {
    console.log('');
    console.log('Reachable-importer failures:');
    for (const failure of summary.extraReachableImporters.slice(0, 20)) {
      const importers = failure.importers
        .slice(0, 4)
        .map(ref => `${ref.importer} (${ref.specifier})`)
        .join('; ');
      console.log(`  ${failure.file} <- ${importers}`);
    }
  }
  printExamples('Missing synthetic file reports', summary.missingFuzzFiles);
  printExamples('Mismatched synthetic file reports', summary.unexpectedFuzzFiles);
  printExamples('Missing synthetic export reports', summary.missingFuzzExports);
  printExamples('Unexpected synthetic export reports', summary.unexpectedUsedFuzzExports);
  printExamples('Synthetic files missing from fix result', summary.fixConformance.syntheticFilesMissingFromFixResult);
  printExamples('Synthetic files still on disk after fix', summary.fixConformance.syntheticFilesStillOnDisk);
  printExamples('Synthetic export modules missing from fix result', summary.fixConformance.syntheticExportModulesMissingFromFixResult);
  printExamples('Synthetic export modules missing on disk after fix', summary.fixConformance.syntheticExportModulesMissingOnDisk);
  printExamples('Synthetic unused exports still in source after fix', summary.fixConformance.syntheticUnusedExportsStillInSource);
  printExamples('Synthetic used exports missing from source after fix', summary.fixConformance.syntheticUsedExportsMissingFromSource);
  printExamples('Synthetic files reported after fix', summary.fixConformance.syntheticFilesReportedAfterFix);
  printExamples('Synthetic unused exports reported after fix', summary.fixConformance.syntheticUnusedExportsReportedAfterFix);
  printExamples('Synthetic used exports reported after fix', summary.fixConformance.syntheticUsedExportsReportedAfterFix);

  if (summary.fuzzFiles.length > 0) {
    printExamples('Synthetic unused sample', summary.fuzzFiles.slice(0, 20));
  }
  if (summary.fuzzExports.length > 0) {
    printExamples(
      'Synthetic unused export sample',
      summary.fuzzExports
        .flatMap(module => module.unused.map(symbol => formatExportRef(module.file, symbol)))
        .slice(0, 20),
    );
  }
  if (options.keepTemp) {
    console.log('');
    console.log(`Temp: ${summary.tempRoot}`);
  }
}

function printExamples(label: string, examples: string[]) {
  if (examples.length === 0) {
    return;
  }
  console.log('');
  console.log(`${label}:`);
  for (const example of examples.slice(0, 20)) {
    console.log(`  ${example}`);
  }
}

function formatMs(value: number): string {
  return `${value.toLocaleString('en-US')}ms`;
}

function formatPatterns(patterns: string[]): string {
  if (patterns.length <= 3) {
    return patterns.join(', ');
  }
  return `${patterns.slice(0, 3).join(', ')}, ... (${patterns.length} total)`;
}

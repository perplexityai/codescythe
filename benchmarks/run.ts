#!/usr/bin/env -S node --experimental-transform-types

const Benchmark = require('benchmark');
const { spawnSync } = require('node:child_process');
const {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');

type FixtureName = 'vscode' | 'grafana' | 'kibana' | 'renovate';
type FixtureSelection = FixtureName | 'all';

type Fixture = {
  name: FixtureName;
  label: string;
  repo: string;
  commit: string;
  markerTarget: string;
  sourceFiles: number;
  benchmarkedFiles: number;
  rawTsFiles: number;
  entry?: string[];
  project?: string[];
  ignore?: string[];
  setup?: 'grafana' | 'kibana';
  extraFiles?: string;
  conformanceImporter?: string;
};

type Options = {
  fixture: FixtureSelection;
  samples: number;
  warmups: number;
  once: boolean;
  skipBuild: boolean;
  skipKnip: boolean;
  codescytheBin?: string;
  knipBin?: string;
  fixturePackageJson?: string;
  fixtureRoot?: string;
  snapshotOutput?: string;
  fuzzFiles: number;
  fuzzExports: number;
  seed: number;
  keepTemp: boolean;
  help: boolean;
};

type Tool = {
  label: string;
  command: string;
  args: string[];
  okStatuses: Set<number>;
  cwd: string;
};

type ResultRow = {
  label: string;
  meanMs: number;
  rme: number;
  samples: number;
  hz: number;
};

type JsonRun = {
  value: any;
  elapsedMs: number;
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

const scriptDir = __dirname;
const repoRoot = path.resolve(scriptDir, '..');
const defaultCodescytheBin = path.join(
  repoRoot,
  'target',
  'release',
  process.platform === 'win32' ? 'codescythe.exe' : 'codescythe',
);

const sourcePatterns = ['**/*.{ts,tsx,mts,cts}'];
const javaScriptSourcePatterns = ['**/*.{ts,tsx,mts,cts,js,jsx,mjs,cjs}'];
const fuzzDirectory = 'codescythe-conformance-fuzz';
const fuzzExportDirectory = `${fuzzDirectory}/exports`;
const kibanaSourceRoots = [
  'src',
  'x-pack',
  'packages',
  'examples',
  'oas_docs',
];
const kibanaSourceRootPatterns = kibanaSourceRoots.map(root => `${root}/**/*.{ts,tsx,mts,cts}`);
const vscodeProjectPatterns = [
  'src/**/*.{ts,tsx,mts,cts}',
  'build/**/*.{ts,tsx,mts,cts}',
  'extensions/**/*.{ts,tsx,mts,cts}',
];
const vscodeEntryPatterns = vscodeProjectPatterns;
const grafanaProjectPatterns = [
  'public/**/*.{ts,tsx,mts,cts}',
  'packages/**/*.{ts,tsx,mts,cts}',
  'scripts/**/*.{ts,tsx,mts,cts}',
  'i18next.config.ts',
  'playwright.config.ts',
  'playwright.storybook.config.ts',
];
const grafanaEntryPatterns = grafanaProjectPatterns;
const renovateBuildEntryPatterns = [
  'lib/config-validator.ts',
  'lib/config/defaults.ts',
  'lib/config/global.ts',
  'lib/config/options/env-options.ts',
  'lib/config/options/index.ts',
  'lib/config/types.ts',
  'lib/config/utils.ts',
  'lib/constants/error-messages.ts',
  'lib/instrumentation/types.ts',
  'lib/logger/err-serializer.ts',
  'lib/logger/index.ts',
  'lib/logger/renovate-logger.ts',
  'lib/logger/types.ts',
  'lib/modules/datasource/common.ts',
  'lib/modules/datasource/index.ts',
  'lib/modules/datasource/npm/types.ts',
  'lib/modules/datasource/types.ts',
  'lib/modules/manager/index.ts',
  'lib/modules/manager/types.ts',
  'lib/modules/platform/bitbucket-server/index.ts',
  'lib/modules/platform/bitbucket/index.ts',
  'lib/modules/platform/gitlab/index.ts',
  'lib/modules/platform/index.ts',
  'lib/modules/platform/types.ts',
  'lib/modules/versioning/generic.ts',
  'lib/modules/versioning/index.ts',
  'lib/modules/versioning/ubuntu/index.ts',
  'lib/proxy.ts',
  'lib/renovate.ts',
  'lib/types/index.ts',
  'lib/util/cache/package/backend.ts',
  'lib/util/cache/package/index.ts',
  'lib/util/cache/repository/types.ts',
  'lib/util/compress.ts',
  'lib/util/exec/common.ts',
  'lib/util/exec/exec-error.ts',
  'lib/util/exec/types.ts',
  'lib/util/git/index.ts',
  'lib/util/host-rules.ts',
  'lib/util/http/github.ts',
  'lib/util/http/gitlab.ts',
  'lib/util/http/types.ts',
  'lib/util/s3.ts',
  'lib/util/string-match.ts',
  'lib/util/timestamp.ts',
  'lib/util/url.ts',
  'lib/workers/global/autodiscover.ts',
  'lib/workers/global/config/parse/index.ts',
  'lib/workers/repository/result.ts',
  'lib/workers/types.ts',
];
const renovateToolEntryPatterns = [
  '.markdownlint-cli2.mjs',
  '**/*.spec.ts',
  '__mocks__/**/*.ts',
  'test/setup.ts',
  'test/to-migrate.ts',
  'tools/check-fenced-code.ts',
  'tools/check-git-version.mjs',
  'tools/check/index.ts',
  'tools/clean-cache.mjs',
  'tools/docker.ts',
  'tools/docs/test/**/*.mjs',
  'tools/generate-docs.ts',
  'tools/generate-imports.mjs',
  'tools/generate-schema.ts',
  'tools/mkdocs.ts',
  'tools/prepare-deps.mjs',
  'tools/prepare-release.ts',
  'tools/publish-release.ts',
  'tools/static-data/generate-distro-info.mjs',
  'tools/static-data/generate-lambda-node-schedule.mjs',
  'tools/static-data/generate-mise-registry.ts',
  'tools/static-data/generate-node-schedule.mjs',
  'tools/sync-module-labels.ts',
  'tools/sync-org-issue-fields.ts',
  'tools/lint/**/*.js',
  'tools/test-shards.ts',
  'tools/validate-schema.ts',
  'tsdown.config.mts',
  'vitest.config.mts',
];
const maxJsonBuffer = 1024 * 1024 * 1024;
const ignorePatterns = [
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
];

const fixtures: Fixture[] = [
  {
    name: 'vscode',
    label: 'VS Code',
    repo: 'microsoft/vscode',
    commit: '9b7643f90393b9ad2c5d5cbbdad70fa928090009',
    markerTarget: '@benchmark_vscode//:package_json',
    sourceFiles: 14689,
    benchmarkedFiles: 9398,
    rawTsFiles: 10213,
    entry: vscodeEntryPatterns,
    project: vscodeProjectPatterns,
    conformanceImporter: 'src/vs/code/electron-main/main.ts',
  },
  {
    name: 'grafana',
    label: 'Grafana',
    repo: 'grafana/grafana',
    commit: '7709dc39cf8ee2de85c38b8943b208adf8a3c47c',
    markerTarget: '@benchmark_grafana//:package_json',
    sourceFiles: 21680,
    benchmarkedFiles: 8358,
    rawTsFiles: 8733,
    entry: grafanaEntryPatterns,
    project: grafanaProjectPatterns,
    extraFiles: '5,955 Go files',
    setup: 'grafana',
    conformanceImporter: 'public/app/index.ts',
  },
  {
    name: 'kibana',
    label: 'Kibana',
    repo: 'elastic/kibana',
    commit: 'd706f62a04af1112db6b4dfef3c94955bdb98250',
    markerTarget: '@benchmark_kibana//:package_json',
    sourceFiles: 110440,
    benchmarkedFiles: 85928,
    rawTsFiles: 87408,
    entry: kibanaSourceRootPatterns,
    project: kibanaSourceRootPatterns,
    ignore: ['**/*.gen.ts'],
    setup: 'kibana',
    conformanceImporter: 'src/core/server/index.ts',
  },
  {
    name: 'renovate',
    label: 'Renovate',
    repo: 'renovatebot/renovate',
    commit: 'b42bb1dc25287ab0b2b328559674e442d3290da9',
    markerTarget: '@benchmark_renovate//:package_json',
    sourceFiles: 3015,
    benchmarkedFiles: 2456,
    rawTsFiles: 2472,
    entry: [...renovateBuildEntryPatterns, ...renovateToolEntryPatterns],
    project: javaScriptSourcePatterns,
    conformanceImporter: 'lib/renovate.ts',
  },
];

let executionRoot: string | undefined;
let outputBase: string | undefined;

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  printHelp();
  process.exit(0);
}

const selectedFixtures = selectFixtures(options.fixture);
if ((options.fixturePackageJson || options.fixtureRoot) && selectedFixtures.length !== 1) {
  throw new Error('--fixture-package-json and --fixture-root require a single --fixture selection');
}
if (options.snapshotOutput && selectedFixtures.length !== 1) {
  throw new Error('--snapshot-output requires a single --fixture selection');
}
const configRoot = mkdtempSync(path.join(tmpdir(), 'codescythe-benchmark-config-'));
const snapshotTempRoot = options.snapshotOutput
  ? mkdtempSync(path.join(testTempRoot(), 'codescythe-fixture-conformance-'))
  : undefined;

try {
  const codescytheBin = resolveCodescytheBin(options);
  const knipBin = options.skipKnip ? undefined : resolveKnipBin(options);

  if (options.snapshotOutput) {
    if (!knipBin) {
      throw new Error('Knip is required when --snapshot-output is used');
    }
    runFixtureConformanceSnapshot({
      fixture: selectedFixtures[0],
      tempRoot: snapshotTempRoot!,
      configRoot,
      codescytheBin,
      knipBin,
      outputPath: options.snapshotOutput,
    });
  } else {
    for (const fixture of selectedFixtures) {
      const fixtureRoot = resolveFixtureRoot(fixture, options);
      prepareFixture(fixture, fixtureRoot);
      const configPath = writeFixtureConfig(configRoot, fixture);
      const knipConfigPath = knipBin
        ? writeKnipCompatibleConfig(configRoot, fixture, false, `${fixture.name}.knip-benchmark.json`)
        : undefined;
      const tools = createTools(fixtureRoot, configPath, codescytheBin, knipBin, knipConfigPath);
      const rows = options.once ? runToolsOnce(tools) : measureTools(tools, options);
      printSummary(fixture, fixtureRoot, options, rows, knipBin);
    }
  }
} finally {
  rmSync(configRoot, { recursive: true, force: true });
  if (snapshotTempRoot && !options.keepTemp) {
    rmSync(snapshotTempRoot, { recursive: true, force: true });
  }
}

function parseArgs(args: string[]): Options {
  const parsed: Options = {
    fixture: 'all',
    samples: 5,
    warmups: 1,
    once: false,
    skipBuild: false,
    skipKnip: false,
    codescytheBin: process.env.CODESCYTHE_BIN,
    knipBin: process.env.KNIP_BIN,
    fixturePackageJson: process.env.BENCHMARK_FIXTURE_PACKAGE_JSON,
    fixtureRoot: process.env.BENCHMARK_FIXTURE_ROOT,
    snapshotOutput: process.env.BENCHMARK_SNAPSHOT_OUTPUT,
    fuzzFiles: 16,
    fuzzExports: 8,
    seed: 0xc0dec7,
    keepTemp: false,
    help: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--') {
      continue;
    } else if (arg === '--fixture') {
      parsed.fixture = parseFixtureSelection(args[++index]);
    } else if (arg === '--samples' || arg === '--runs') {
      parsed.samples = parsePositiveInt(args[++index], arg);
    } else if (arg === '--warmups') {
      parsed.warmups = parseNonNegativeInt(args[++index], '--warmups');
    } else if (arg === '--once') {
      parsed.once = true;
    } else if (arg === '--skip-build') {
      parsed.skipBuild = true;
    } else if (arg === '--skip-knip') {
      parsed.skipKnip = true;
    } else if (arg === '--codescythe-bin') {
      parsed.codescytheBin = requireValue(args[++index], '--codescythe-bin');
    } else if (arg === '--knip-bin') {
      parsed.knipBin = requireValue(args[++index], '--knip-bin');
    } else if (arg === '--fixture-package-json') {
      parsed.fixturePackageJson = requireValue(args[++index], '--fixture-package-json');
    } else if (arg === '--fixture-root') {
      parsed.fixtureRoot = requireValue(args[++index], '--fixture-root');
    } else if (arg === '--snapshot-output') {
      parsed.snapshotOutput = requireValue(args[++index], '--snapshot-output');
    } else if (arg === '--fuzz-files') {
      parsed.fuzzFiles = parseNonNegativeInt(args[++index], '--fuzz-files');
    } else if (arg === '--fuzz-exports') {
      parsed.fuzzExports = parseNonNegativeInt(args[++index], '--fuzz-exports');
    } else if (arg === '--seed') {
      parsed.seed = parseSeed(args[++index], '--seed');
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

function parseFixtureSelection(value: string | undefined): FixtureSelection {
  const fixture = requireValue(value, '--fixture');
  if (
    fixture === 'all' ||
    fixture === 'vscode' ||
    fixture === 'grafana' ||
    fixture === 'kibana' ||
    fixture === 'renovate'
  ) {
    return fixture;
  }
  throw new Error(`--fixture must be one of: all, vscode, grafana, kibana, renovate`);
}

function requireValue(value: string | undefined, flag: string): string {
  if (!value) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function parsePositiveInt(value: string | undefined, flag: string): number {
  return parseMinInt(value, flag, 1);
}

function parseNonNegativeInt(value: string | undefined, flag: string): number {
  return parseMinInt(value, flag, 0);
}

function parseMinInt(value: string | undefined, flag: string, min: number): number {
  const parsed = Number.parseInt(requireValue(value, flag), 10);
  if (!Number.isSafeInteger(parsed) || parsed < min) {
    throw new Error(`${flag} must be an integer greater than or equal to ${min}`);
  }
  return parsed;
}

function parseSeed(value: string | undefined, flag: string): number {
  const raw = requireValue(value, flag);
  const parsed = raw.startsWith('0x')
    ? Number.parseInt(raw.slice(2), 16)
    : Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed) || parsed < 0 || parsed > 0xffffffff) {
    throw new Error(`${flag} must be a 32-bit unsigned integer`);
  }
  return parsed >>> 0;
}

function selectFixtures(selection: FixtureSelection): Fixture[] {
  if (selection === 'all') {
    return fixtures;
  }
  return fixtures.filter(fixture => fixture.name === selection);
}

function testTempRoot(): string {
  return process.env.TEST_TMPDIR || tmpdir();
}

function writeFixtureConfig(
  directory: string,
  fixture: Fixture,
  includeFuzz = false,
  fileName = `${fixture.name}.json`,
): string {
  const configPath = path.join(directory, fileName);
  const project = [...(fixture.project ?? sourcePatterns)];
  if (includeFuzz) {
    project.push(`${fuzzDirectory}/**/*.{ts,tsx,mts,cts}`);
  }
  writeJson(configPath, {
    entry: fixture.entry ?? project,
    project,
    ignore: [...ignorePatterns, ...(fixture.ignore ?? [])],
    testFilePatterns: [],
    includeEntryExports: true,
    ignoreExportsUsedInFile: false,
  });
  return configPath;
}

function writeKnipConfig(
  directory: string,
  fixture: Fixture,
  disabledKnipPlugins: string[],
): string {
  return writeKnipCompatibleConfig(directory, fixture, true, `${fixture.name}.knip.json`, disabledKnipPlugins);
}

function writeKnipCompatibleConfig(
  directory: string,
  fixture: Fixture,
  includeFuzz: boolean,
  fileName: string,
  disabledKnipPlugins: string[] = [],
): string {
  const configPath = writeFixtureConfig(directory, fixture, includeFuzz, fileName);
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  delete config.testFilePatterns;
  for (const pluginName of disabledKnipPlugins) {
    config[pluginName] = false;
  }
  writeJson(configPath, config);
  return configPath;
}

function writeFuzzFixConfig(directory: string, fixture: Fixture): string {
  if (!fixture.conformanceImporter) {
    throw new Error(`${fixture.name} does not define a conformance importer`);
  }
  const configPath = path.join(directory, `${fixture.name}.fix.json`);
  writeJson(configPath, {
    entry: [fixture.conformanceImporter],
    project: [
      fixture.conformanceImporter,
      `${fuzzDirectory}/**/*.{ts,tsx,mts,cts}`,
    ],
    ignore: [...ignorePatterns, ...(fixture.ignore ?? [])],
    testFilePatterns: [],
    includeEntryExports: true,
    ignoreExportsUsedInFile: false,
  });
  return configPath;
}

function prepareFixture(fixture: Fixture, fixtureRoot: string) {
  if (fixture.setup === 'grafana') {
    const tsconfigDir = path.join(
      fixtureRoot,
      'node_modules',
      '@grafana',
      'tsconfig',
    );
    mkdirSync(tsconfigDir, { recursive: true });
    writeJson(path.join(tsconfigDir, 'package.json'), {
      name: '@grafana/tsconfig',
      version: '0.0.0',
      main: 'tsconfig.json',
    });
    writeJson(path.join(tsconfigDir, 'tsconfig.json'), {
      compilerOptions: {
        jsx: 'react-jsx',
        moduleResolution: 'bundler',
        resolveJsonModule: true,
      },
    });

    const pluginConfigsDir = path.join(
      fixtureRoot,
      'node_modules',
      '@grafana',
      'plugin-configs',
    );
    mkdirSync(pluginConfigsDir, { recursive: true });
    writeJson(path.join(pluginConfigsDir, 'tsconfig.json'), {
      compilerOptions: {
        allowImportingTsExtensions: true,
        customConditions: ['@grafana-app/source'],
        jsx: 'react-jsx',
        moduleResolution: 'bundler',
        resolveJsonModule: true,
      },
    });
  }
  if (fixture.setup === 'kibana') {
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
}

function writeJson(filePath: string, value: unknown) {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function createTools(
  fixtureRoot: string,
  configPath: string,
  codescytheBin: string,
  knipBin: string | undefined,
  knipConfigPath = configPath,
): Tool[] {
  const tools: Tool[] = [
    {
      label: 'codescythe',
      command: codescytheBin,
      args: [
        '--json',
        '--directory',
        fixtureRoot,
        '--config',
        configPath,
      ],
      okStatuses: new Set([0, 1]),
      cwd: repoRoot,
    },
  ];

  if (knipBin) {
    tools.push({
      label: 'knip',
      command: knipBin,
      args: [
        '--no-progress',
        '--no-config-hints',
        '--no-exit-code',
        '--reporter',
        'json',
        '--include',
        'files,exports,types',
        '--config',
        knipConfigPath,
      ],
      okStatuses: new Set([0]),
      cwd: fixtureRoot,
    });
  }

  return tools;
}

function runFixtureConformanceSnapshot(request: {
  fixture: Fixture;
  tempRoot: string;
  configRoot: string;
  codescytheBin: string;
  knipBin: string;
  outputPath: string;
}) {
  const sourceFixtureRoot = resolveFixtureRoot(request.fixture, {
    ...options,
    fixture: request.fixture.name,
  });
  const fixtureRoot = prepareMutableFixture(request.fixture, sourceFixtureRoot, request.tempRoot);
  prepareFixture(request.fixture, fixtureRoot);

  const fuzzFiles = writeSyntheticUnusedFiles(fixtureRoot, options.fuzzFiles, options.seed);
  const fuzzExports = writeSyntheticUnusedExports(
    request.fixture,
    fixtureRoot,
    options.fuzzExports,
    options.seed,
  );

  const codescytheConfig = writeFixtureConfig(
    request.configRoot,
    request.fixture,
    true,
    `${request.fixture.name}.codescythe.json`,
  );
  const fixConfig = writeFuzzFixConfig(request.configRoot, request.fixture);
  const knipConfig = writeKnipConfig(
    request.configRoot,
    request.fixture,
    readKnipPluginNames(request.knipBin),
  );

  const codescythe = runJson('codescythe', toolForExecutable(request.codescytheBin), [
    '--json',
    '--directory',
    fixtureRoot,
    '--config',
    codescytheConfig,
  ], [0, 1]);
  const knip = runJson('knip', toolForExecutable(request.knipBin), [
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
  const missingFuzzFiles = fuzzFiles.filter(file => !codescytheUnused.has(file) || !knipUnused.has(file));
  const unexpectedFuzzFiles = fuzzFiles.filter(file => codescytheUnused.has(file) !== knipUnused.has(file));
  const missingFuzzExports = fuzzExports
    .flatMap(module => module.unused.map(symbol => formatExportRef(module.file, symbol)))
    .filter(ref => !codescytheUnusedExports.has(ref));
  const unexpectedUsedFuzzExports = fuzzExports
    .map(module => formatExportRef(module.file, module.used))
    .filter(ref => codescytheUnusedExports.has(ref));
  const fixConformance = runFixConformance({
    codescytheBin: request.codescytheBin,
    fixtureRoot,
    codescytheConfig: fixConfig,
    fuzzFiles,
    fuzzExports,
  });

  const snapshot = createConformanceSnapshot({
    fixture: request.fixture,
    codescythe,
    codescytheUnused,
    codescytheUnusedExports,
    knip,
    knipUnused,
    missingFromCodescythe,
    extraInCodescythe,
    fuzzFiles,
    fuzzExports,
    missingFuzzFiles,
    unexpectedFuzzFiles,
    missingFuzzExports,
    unexpectedUsedFuzzExports,
    fixConformance,
  });
  const outputPath = resolveOutputPath(request.outputPath);
  writeJson(outputPath, snapshot);
  printConformanceSummary({
    fixture: request.fixture,
    sourceFixtureRoot,
    fixtureRoot,
    codescythe,
    codescytheUnused,
    codescytheUnusedExports,
    knip,
    knipUnused,
    missingFromCodescythe,
    extraInCodescythe,
    fuzzFiles,
    fuzzExports,
    missingFuzzFiles,
    unexpectedFuzzFiles,
    missingFuzzExports,
    unexpectedUsedFuzzExports,
    fixConformance,
    outputPath,
  });
}

function prepareMutableFixture(fixture: Fixture, sourceFixtureRoot: string, directory: string): string {
  const fixtureRoot = path.join(directory, fixture.name);
  cpSync(sourceFixtureRoot, fixtureRoot, {
    recursive: true,
    dereference: true,
    filter: source => {
      const relative = normalizeFixturePath(path.relative(sourceFixtureRoot, source));
      return relative !== '.git' && relative !== 'node_modules';
    },
  });
  sanitizeRootManifest(fixtureRoot);
  if (options.keepTemp) {
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
  fixture: Fixture,
  fixtureRoot: string,
  count: number,
  seed: number,
): SyntheticExportModule[] {
  if (count === 0) {
    return [];
  }
  if (!fixture.conformanceImporter) {
    throw new Error(`${fixture.name} does not define a conformance importer`);
  }

  const importer = fixture.conformanceImporter;
  const importerPath = path.join(fixtureRoot, importer);
  assertExecutable(importerPath, `${fixture.label} conformance importer`);
  mkdirSync(path.join(fixtureRoot, fuzzExportDirectory), { recursive: true });

  let state = (seed ^ 0x9e3779b9) >>> 0;
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
    importLines.push(`import { ${used} } from '${relativeImportSpecifier(importer, file)}';`);
    useLines.push(`void ${used};`);
    modules.push({ file, used, unused });
  }

  const original = readFileSync(importerPath, 'utf8');
  const injected = [
    ...importLines,
    ...useLines,
    '',
  ].join('\n');
  if (original.startsWith('#!')) {
    const firstNewline = original.indexOf('\n');
    const shebang = firstNewline === -1 ? original : original.slice(0, firstNewline + 1);
    const rest = firstNewline === -1 ? '' : original.slice(firstNewline + 1);
    writeFileSync(importerPath, `${shebang}${injected}${rest}`);
    return modules;
  }
  writeFileSync(
    importerPath,
    [
      injected,
      original,
    ].join('\n'),
  );
  return modules;
}

function relativeImportSpecifier(importer: string, imported: string): string {
  const importedWithoutExtension = imported.replace(/\.[^.]+$/, '');
  let specifier = path.posix.relative(path.posix.dirname(importer), importedWithoutExtension);
  if (!specifier.startsWith('.')) {
    specifier = `./${specifier}`;
  }
  return specifier;
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
  tool: Pick<Tool, 'command' | 'args'>,
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

function toolForExecutable(command: string): Pick<Tool, 'command' | 'args'> {
  return { command, args: [] };
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

function createConformanceSnapshot(summary: {
  fixture: Fixture;
  codescythe: JsonRun;
  codescytheUnused: Set<string>;
  codescytheUnusedExports: Set<string>;
  knip: JsonRun;
  knipUnused: Set<string>;
  missingFromCodescythe: string[];
  extraInCodescythe: string[];
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
  return {
    fixture: summary.fixture.name,
    repo: summary.fixture.repo,
    commit: summary.fixture.commit,
    seed: `0x${options.seed.toString(16)}`,
    config: {
      entry: summary.fixture.entry ?? summary.fixture.project ?? sourcePatterns,
      project: [
        ...(summary.fixture.project ?? sourcePatterns),
        `${fuzzDirectory}/**/*.{ts,tsx,mts,cts}`,
      ],
      testFilePatterns: [],
      includeEntryExports: true,
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
      syntheticFilesMissingFromEitherTool: summary.missingFuzzFiles,
      syntheticFilesWithMismatchedReports: summary.unexpectedFuzzFiles,
      syntheticUnusedExportsMissingFromCodescythe: summary.missingFuzzExports,
      syntheticUsedExportsReportedByCodescythe: summary.unexpectedUsedFuzzExports,
    },
  };
}

function printConformanceSummary(summary: {
  fixture: Fixture;
  sourceFixtureRoot: string;
  fixtureRoot: string;
  codescythe: JsonRun;
  codescytheUnused: Set<string>;
  codescytheUnusedExports: Set<string>;
  knip: JsonRun;
  knipUnused: Set<string>;
  missingFromCodescythe: string[];
  extraInCodescythe: string[];
  fuzzFiles: string[];
  fuzzExports: SyntheticExportModule[];
  missingFuzzFiles: string[];
  unexpectedFuzzFiles: string[];
  missingFuzzExports: string[];
  unexpectedUsedFuzzExports: string[];
  fixConformance: FixConformance;
  outputPath: string;
}) {
  const analysisCounters = summary.codescythe.value.counters ?? {};
  const fixCounters = summary.fixConformance.codescytheFix.value ?? {};
  const postFixCounters = summary.fixConformance.postFixCodescythe.value.counters ?? {};

  console.log(`Fixture: ${summary.fixture.label} (${summary.fixture.repo} @ ${summary.fixture.commit.slice(0, 12)})`);
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
  console.log(`Config entry: ${formatPatterns(summary.fixture.entry ?? summary.fixture.project ?? sourcePatterns)}`);
  console.log(`Config project: ${formatPatterns(summary.fixture.project ?? sourcePatterns)}`);
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
  console.log(`  Synthetic files missing from either tool: ${summary.missingFuzzFiles.length}`);
  console.log(`  Synthetic files with mismatched reports: ${summary.unexpectedFuzzFiles.length}`);
  console.log(`  Synthetic unused exports missing from Codescythe: ${summary.missingFuzzExports.length}`);
  console.log(`  Synthetic used exports reported by Codescythe: ${summary.unexpectedUsedFuzzExports.length}`);
  console.log(`Snapshot: ${summary.outputPath}`);
  console.log('');
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

  assertExecutable(defaultCodescytheBin, 'Codescythe release binary');
  return defaultCodescytheBin;
}

function resolveKnipBin(parsed: Options): string | undefined {
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
  if (canRun(localBin, ['--version'])) {
    return localBin;
  }

  if (canRun('knip', ['--version'])) {
    return 'knip';
  }

  return undefined;
}

function resolveExistingPath(input: string, label: string): string {
  const candidates = resolvePathCandidates(input);
  const found = candidates.find(candidate => existsSync(candidate));
  if (!found) {
    throw new Error(`${label} not found at ${input}; tried: ${candidates.join(', ')}`);
  }
  return realpathSync(found);
}

function resolveOutputPath(input: string): string {
  if (path.isAbsolute(input)) {
    return input;
  }
  const execroot = executionRootFromCwd();
  if (execroot && input.startsWith(`bazel-out${path.sep}`)) {
    return path.join(execroot, input);
  }
  return path.resolve(input);
}

function resolvePathCandidates(input: string): string[] {
  const candidates = new Set<string>();
  if (path.isAbsolute(input)) {
    candidates.add(input);
  } else {
    candidates.add(path.resolve(input));
    const execroot = executionRootFromCwd();
    if (execroot) {
      candidates.add(path.join(execroot, input));
    }
    for (const root of runfilesRoots()) {
      candidates.add(path.join(root, input));
      candidates.add(path.join(root, '_main', input));
    }
    candidates.add(path.join(repoRoot, input));
  }
  return [...candidates];
}

function executionRootFromCwd(): string | undefined {
  const marker = `${path.sep}bazel-out${path.sep}`;
  const index = process.cwd().indexOf(marker);
  return index === -1 ? undefined : process.cwd().slice(0, index);
}

function runfilesRoots(): string[] {
  return [
    process.env.RUNFILES_DIR,
    process.env.TEST_SRCDIR,
  ].filter((value): value is string => !!value);
}

function assertExecutable(command: string, label: string) {
  if (!existsSync(command)) {
    throw new Error(`${label} not found at ${command}`);
  }
}

function canRun(command: string, args: string[]) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: 'ignore',
  });
  return result.status === 0;
}

function resolveFixtureRoot(fixture: Fixture, parsed: Options): string {
  if (parsed.fixtureRoot) {
    const fixtureRoot = resolveExistingPath(parsed.fixtureRoot, `${fixture.label} fixture root`);
    assertExecutable(path.join(fixtureRoot, 'package.json'), `${fixture.label} fixture package.json`);
    return realpathSync(fixtureRoot);
  }
  if (parsed.fixturePackageJson) {
    const packageJson = resolveExistingPath(parsed.fixturePackageJson, `${fixture.label} fixture package.json`);
    return realpathSync(path.dirname(packageJson));
  }

  const markerPath = bazelStdout([
    'cquery',
    fixture.markerTarget,
    '--output=files',
    '--noshow_progress',
  ])
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(Boolean)
    .find(line => line.endsWith('package.json'));

  if (!markerPath) {
    throw new Error(`Bazel did not return package.json for ${fixture.markerTarget}`);
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

function measureTools(tools: Tool[], parsed: Options): ResultRow[] {
  for (const tool of tools) {
    for (let index = 0; index < parsed.warmups; index += 1) {
      runTool(tool);
    }
  }

  const suite = new Benchmark.Suite();
  for (const tool of tools) {
    suite.add(tool.label, {
      minSamples: parsed.samples,
      fn: () => runTool(tool),
    });
  }

  suite.run({ async: false });

  const rows: ResultRow[] = [];
  suite.forEach(bench => {
    if (bench.error) {
      throw bench.error;
    }
    rows.push({
      label: bench.name,
      meanMs: bench.stats.mean * 1000,
      rme: bench.stats.rme,
      samples: bench.stats.sample.length,
      hz: bench.hz,
    });
  });
  return rows;
}

function runToolsOnce(tools: Tool[]): ResultRow[] {
  return tools.map(tool => {
    const elapsedMs = runTool(tool);
    return {
      label: tool.label,
      meanMs: elapsedMs,
      rme: 0,
      samples: 1,
      hz: elapsedMs === 0 ? Number.POSITIVE_INFINITY : 1000 / elapsedMs,
    };
  });
}

function runTool(tool: Tool): number {
  const started = Date.now();
  const result = spawnSync(tool.command, tool.args, {
    cwd: tool.cwd,
    encoding: 'utf8',
    env: {
      ...process.env,
      CI: '1',
      NO_COLOR: '1',
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });

  if (!tool.okStatuses.has(result.status ?? -1)) {
    throw new Error(
      `${tool.label} failed with exit code ${result.status ?? 'unknown'}:\n${result.stderr}`,
    );
  }
  return Date.now() - started;
}

function printSummary(
  fixture: Fixture,
  fixtureRoot: string,
  parsed: Options,
  rows: ResultRow[],
  knipBin: string | undefined,
) {
  console.log(`Fixture: ${fixture.label} (${fixture.repo} @ ${fixture.commit.slice(0, 12)})`);
  console.log(`Root: ${fixtureRoot}`);
  console.log(`Corpus: ${formatCorpus(fixture)}`);
  console.log(`Config: entry ${formatPatterns(fixture.entry ?? fixture.project ?? sourcePatterns)}`);
  console.log(`Config: project ${formatPatterns(fixture.project ?? sourcePatterns)}`);
  if (parsed.once) {
    console.log('Runs: 1 functional smoke run');
  } else {
    console.log(`Runs: ${parsed.samples} minimum samples, ${parsed.warmups} warmup runs`);
  }
  console.log('');
  console.log(formatTable(rows));

  if (parsed.skipKnip) {
    console.log('\nKnip: skipped by --skip-knip');
  } else if (!knipBin) {
    console.log('\nKnip: skipped; run pnpm install, set KNIP_BIN, or put knip on PATH to compare.');
  }

  console.log('');
}

function formatCorpus(fixture: Fixture) {
  const parts = [
    `${formatCount(fixture.sourceFiles)} source files`,
    `${formatCount(fixture.benchmarkedFiles)} benchmarked source files`,
    `${formatCount(fixture.rawTsFiles)} raw TS/TSX files`,
  ];
  if (fixture.extraFiles) {
    parts.push(fixture.extraFiles);
  }
  return parts.join(', ');
}

function formatCount(value: number) {
  return value.toLocaleString('en-US');
}

function formatPatterns(patterns: string[]) {
  if (patterns.length <= 3) {
    return patterns.join(', ');
  }
  return `${patterns.slice(0, 3).join(', ')}, ... (${patterns.length} total)`;
}

function formatTable(rows: ResultRow[]) {
  const table = [
    ['tool', 'mean', 'rme', 'samples', 'ops/sec'],
    ...rows.map(row => [
      row.label,
      formatMs(row.meanMs),
      `+/-${row.rme.toFixed(2)}%`,
      row.samples.toString(),
      row.hz.toFixed(row.hz >= 100 ? 1 : 2),
    ]),
  ];
  const widths = table[0].map((_, column) =>
    Math.max(...table.map(row => row[column].length)),
  );

  return table
    .map((row, index) => {
      const line = row
        .map((cell, column) => cell.padEnd(widths[column]))
        .join('  ');
      return index === 0
        ? `${line}\n${widths.map(width => '-'.repeat(width)).join('  ')}`
        : line;
    })
    .join('\n');
}

function formatMs(value: number) {
  return `${value.toFixed(1)}ms`;
}

function printHelp() {
  console.log(`Usage: node --experimental-transform-types benchmarks/run.ts [options]

Options:
  --fixture <name>       Fixture to benchmark: all, vscode, grafana, kibana, renovate (default: all)
  --samples <n>          Minimum Benchmark.js samples per tool (default: 5)
  --warmups <n>          Warmup runs per tool (default: 1)
  --once                 Run each selected tool once instead of benchmarking
  --skip-build           Use target/release/codescythe without rebuilding
  --skip-knip            Measure Codescythe only
  --codescythe-bin <bin> Use a specific Codescythe binary
  --knip-bin <bin>       Use a specific Knip binary
  --fixture-package-json <file>
                          Use a Bazel-provided fixture package.json instead of cquery
  --fixture-root <dir>   Use a specific fixture directory instead of cquery
  --snapshot-output <file>
                          Write a stable conformance snapshot JSON file
  --fuzz-files <n>       Synthetic unused files for snapshot mode (default: 16)
  --fuzz-exports <n>     Synthetic reachable modules with unused exports (default: 8)
  --seed <n>             Synthetic fixture seed (default: 0xc0dec7)
  --keep-temp            Keep copied conformance fixture when snapshotting
  -h, --help             Show this help text
`);
}

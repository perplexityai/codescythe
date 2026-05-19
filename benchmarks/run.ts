#!/usr/bin/env -S node --experimental-transform-types

const Benchmark = require('benchmark');
const { spawnSync } = require('node:child_process');
const {
  existsSync,
  mkdirSync,
  mkdtempSync,
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
    benchmarkedFiles: 9537,
    rawTsFiles: 10213,
  },
  {
    name: 'grafana',
    label: 'Grafana',
    repo: 'grafana/grafana',
    commit: '7709dc39cf8ee2de85c38b8943b208adf8a3c47c',
    markerTarget: '@benchmark_grafana//:package_json',
    sourceFiles: 21680,
    benchmarkedFiles: 8701,
    rawTsFiles: 8733,
    extraFiles: '5,955 Go files',
    setup: 'grafana',
  },
  {
    name: 'kibana',
    label: 'Kibana',
    repo: 'elastic/kibana',
    commit: 'd706f62a04af1112db6b4dfef3c94955bdb98250',
    markerTarget: '@benchmark_kibana//:package_json',
    sourceFiles: 110440,
    benchmarkedFiles: 86056,
    rawTsFiles: 87408,
    entry: [
      'src/core/server/index.ts',
      'src/core/public/index.ts',
      'src/platform/packages/shared/kbn-config-schema/index.ts',
      'x-pack/platform/plugins/shared/security/server/index.ts',
    ],
    ignore: ['**/*.gen.ts'],
    setup: 'kibana',
  },
  {
    name: 'renovate',
    label: 'Renovate',
    repo: 'renovatebot/renovate',
    commit: 'b42bb1dc25287ab0b2b328559674e442d3290da9',
    markerTarget: '@benchmark_renovate//:package_json',
    sourceFiles: 3015,
    benchmarkedFiles: 2488,
    rawTsFiles: 2472,
    entry: [
      '.markdownlint-cli2.mjs',
      'lib/renovate.ts',
      'lib/config-validator.ts',
      'tools/check-fenced-code.ts',
      'tools/check-git-version.mjs',
      'tools/check/index.ts',
      'tools/clean-cache.mjs',
      'tools/docker.ts',
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
      'tools/test-shards.ts',
      'tools/validate-schema.ts',
      'tsdown.config.mts',
      'vitest.config.mts',
    ],
    project: javaScriptSourcePatterns,
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
const configRoot = mkdtempSync(path.join(tmpdir(), 'codescythe-benchmark-config-'));

try {
  const codescytheBin = resolveCodescytheBin(options);
  const knipBin = options.skipKnip ? undefined : resolveKnipBin(options);

  for (const fixture of selectedFixtures) {
    const fixtureRoot = resolveFixtureRoot(fixture, options);
    prepareFixture(fixture, fixtureRoot);
    const configPath = writeFixtureConfig(configRoot, fixture);
    const tools = createTools(fixtureRoot, configPath, codescytheBin, knipBin);
    const rows = options.once ? runToolsOnce(tools) : measureTools(tools, options);
    printSummary(fixture, fixtureRoot, options, rows, knipBin);
  }
} finally {
  rmSync(configRoot, { recursive: true, force: true });
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
      parsed.codescytheBin = path.resolve(requireValue(args[++index], '--codescythe-bin'));
    } else if (arg === '--knip-bin') {
      parsed.knipBin = path.resolve(requireValue(args[++index], '--knip-bin'));
    } else if (arg === '--fixture-package-json') {
      parsed.fixturePackageJson = path.resolve(requireValue(args[++index], '--fixture-package-json'));
    } else if (arg === '--fixture-root') {
      parsed.fixtureRoot = path.resolve(requireValue(args[++index], '--fixture-root'));
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

function selectFixtures(selection: FixtureSelection): Fixture[] {
  if (selection === 'all') {
    return fixtures;
  }
  return fixtures.filter(fixture => fixture.name === selection);
}

function writeFixtureConfig(directory: string, fixture: Fixture): string {
  const configPath = path.join(directory, `${fixture.name}.json`);
  const project = fixture.project ?? sourcePatterns;
  writeJson(configPath, {
    entry: fixture.entry ?? project,
    project,
    ignore: [...ignorePatterns, ...(fixture.ignore ?? [])],
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
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function createTools(
  fixtureRoot: string,
  configPath: string,
  codescytheBin: string,
  knipBin: string | undefined,
): Tool[] {
  const tools: Tool[] = [
    {
      label: 'codescythe',
      command: codescytheBin,
      args: [
        '--json',
        '--compact-json',
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
        configPath,
      ],
      okStatuses: new Set([0]),
      cwd: fixtureRoot,
    });
  }

  return tools;
}

function resolveCodescytheBin(parsed: Options): string {
  if (parsed.codescytheBin) {
    assertExecutable(parsed.codescytheBin, 'Codescythe binary');
    return parsed.codescytheBin;
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
    assertExecutable(parsed.knipBin, 'Knip binary');
    return parsed.knipBin;
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
    assertExecutable(path.join(parsed.fixtureRoot, 'package.json'), `${fixture.label} fixture package.json`);
    return realpathSync(parsed.fixtureRoot);
  }
  if (parsed.fixturePackageJson) {
    assertExecutable(parsed.fixturePackageJson, `${fixture.label} fixture package.json`);
    return realpathSync(path.dirname(parsed.fixturePackageJson));
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
  -h, --help             Show this help text
`);
}

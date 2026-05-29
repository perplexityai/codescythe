#!/usr/bin/env -S node --experimental-transform-types

const React = require('react');
const { renderToStaticMarkup } = require('react-dom/server');
const {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} = require('node:fs');
const path = require('node:path');

type NavItem = {
  href: string;
  label: string;
};

type Section = {
  id: string;
  title: string;
};

type Page = {
  slug: string;
  title: string;
  eyebrow: string;
  description: string;
  sections: Section[];
  body: React.ReactNode;
};

type DocLink = {
  href: string;
  title: string;
  description: string;
};

const h = React.createElement;

type BuildOptions = {
  outDir?: string;
  quiet?: boolean;
  rootDir?: string;
};

type BuildPaths = {
  assetDir: string;
  docsDir: string;
  publicDir: string;
  rootDir: string;
  srcDir: string;
  vendorDir: string;
};

function workspaceRoot() {
  return process.env.BUILD_WORKSPACE_DIRECTORY ?? process.cwd();
}

function resolveBuildPaths(options: BuildOptions = {}): BuildPaths {
  const rootDir = path.resolve(options.rootDir ?? workspaceRoot());
  const docsDir = path.join(rootDir, 'docs');
  const srcDir = path.join(docsDir, 'src');
  const publicDir = options.outDir
    ? path.resolve(rootDir, options.outDir)
    : path.join(docsDir, 'public');
  const assetDir = path.join(publicDir, 'assets');
  const vendorDir = path.join(publicDir, 'vendor');

  return { assetDir, docsDir, publicDir, rootDir, srcDir, vendorDir };
}

const primaryNav: NavItem[] = [
  { href: './getting-started/', label: 'Docs' },
];

const homeCards: DocLink[] = [
  {
    href: './getting-started/',
    title: 'Getting Started',
    description: 'Install Codescythe, add a minimal config, run analysis, and apply supported cleanup safely.',
  },
  {
    href: './configuration/',
    title: 'Configuration',
    description: 'Understand entry files, project globs, test file patterns, aliases, ignores, and unresolved imports.',
  },
  {
    href: './features/',
    title: 'Features',
    description: 'Review the analyzer, fix mode, verbose JSON, export explanations, and doctor diagnostics.',
  },
  {
    href: './queries/',
    title: 'Dependency Queries',
    description: 'Trace somepath, somepaths, and allpaths results with text, JSON, Mermaid, or SVG output.',
  },
  {
    href: './reports/',
    title: 'Reports',
    description: 'Read analyzer JSON, doctor warnings, unresolved import diagnostics, and explain-export decisions.',
  },
  {
    href: './troubleshooting/',
    title: 'Troubleshooting',
    description: 'Use doctor and verbose diagnostics to diagnose empty scans, unresolved imports, and risky ignores.',
  },
  {
    href: './performance/',
    title: 'Performance',
    description: 'See the benchmark story, what Codescythe optimizes for, and how to tune large repository runs.',
  },
];

function text(value: string) {
  return value;
}

function CodeBlock({ children, language = 'sh' }: { children: string; language?: string }) {
  return h(
    'pre',
    { className: 'code-block', 'data-language': language },
    h('code', null, children.trim()),
  );
}

function Callout({ title, children }: { title: string; children: React.ReactNode }) {
  return h(
    'aside',
    { className: 'callout' },
    h('strong', null, title),
    h('div', null, children),
  );
}

function QueryFixtureSample({
  command,
  description,
  mermaid,
  title,
}: {
  command: string;
  description: string;
  mermaid: string;
  title: string;
}) {
  return h(
    'article',
    { className: 'query-sample' },
    h('h3', null, title),
    h('p', null, description),
    h(CodeBlock, null, command),
    h(CodeBlock, { language: 'mermaid' }, mermaid),
  );
}

function FieldTable({
  rows,
}: {
  rows: Array<{
    example?: string;
    field: string;
    notes: string;
    purpose: string;
    type?: string;
    values?: string;
  }>;
}) {
  return h(
    'div',
    { className: 'field-table', role: 'table', 'aria-label': 'Configuration fields' },
    rows.map((row) =>
      h(
        'div',
        { className: 'field-row', role: 'row', key: row.field },
        h('div', { className: 'field-name', role: 'cell' }, h('code', null, row.field)),
        h(
          'div',
          { role: 'cell' },
          h('strong', null, row.purpose),
          row.type && h('p', { className: 'field-meta' }, h('span', null, 'Type'), row.type),
          row.values && h('p', { className: 'field-meta' }, h('span', null, 'Values'), row.values),
          h('p', null, row.notes),
          row.example &&
            h(
              'pre',
              { className: 'field-example' },
              h('code', null, row.example.trim()),
            ),
        ),
      ),
    ),
  );
}

function Steps({ items }: { items: Array<{ title: string; body: React.ReactNode }> }) {
  return h(
    'ol',
    { className: 'steps' },
    items.map((item) =>
      h('li', { key: item.title }, h('h3', null, item.title), h('div', null, item.body)),
    ),
  );
}

function PageSection({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: React.ReactNode;
}) {
  return h('section', { className: 'doc-section', id }, h('h2', null, title), children);
}

function InlineLink({ href, children }: { href: string; children: React.ReactNode }) {
  return h('a', { href, className: 'inline-link' }, children);
}

function HeroConsole() {
  return h(
    'div',
    { className: 'terminal-window hero-terminal', 'aria-hidden': 'true' },
    h('div', { className: 'window-bar' }, h('span'), h('span'), h('span')),
    h(
      'pre',
      null,
      h(
        'code',
        null,
        `$ codescythe --fix --json
{
  "removedFiles": ["src/legacy/dead-view.ts"],
  "removedExports": 18,
  "unresolved": []
}`,
      ),
    ),
  );
}

function HomePage() {
  return h(
    'main',
    { id: 'top' },
    h(
      'section',
      { className: 'hero', 'aria-labelledby': 'hero-title' },
      h('div', { className: 'hero-visual', 'aria-hidden': 'true' }, HeroConsole()),
      h(
        'div',
        { className: 'hero-content' },
        h('p', { className: 'eyebrow' }, 'TypeScript and JavaScript dead-code cleanup'),
        h('h1', { id: 'hero-title' }, 'Codescythe'),
        h(
          'p',
          { className: 'hero-copy' },
          'A focused analyzer and remover that starts from explicit entry points, follows the import/export graph, and reports unused files and exports with deterministic behavior.',
        ),
        h(
          'div',
          { className: 'hero-actions', 'aria-label': 'Primary actions' },
          h('a', { className: 'button button-primary', href: './getting-started/' }, 'Start with Codescythe'),
          h('a', { className: 'button button-secondary', href: './configuration/' }, 'Read configuration'),
        ),
      ),
    ),
    h(
      'section',
      { className: 'summary-strip', 'aria-label': 'Codescythe summary' },
      h('div', { className: 'summary-item' }, h('span', null, 'Scope'), h('strong', null, 'Files, exports, unresolved imports')),
      h('div', { className: 'summary-item' }, h('span', null, 'Runtime'), h('strong', null, 'Rust core, CLI, Node-API packages')),
      h('div', { className: 'summary-item' }, h('span', null, 'Fit'), h('strong', null, 'Known source boundaries and repeatable cleanup')),
    ),
    h(
      'section',
      { className: 'section section-light' },
      h(
        'div',
        { className: 'section-inner intro-grid' },
        h(
          'div',
          null,
          h('p', { className: 'eyebrow' }, 'Documentation'),
          h('h2', null, 'A small contract for large source trees'),
          h(
            'p',
            null,
            'Codescythe intentionally avoids framework plugins and dependency-audit breadth. It asks you to declare the source boundary, then it gives you a reviewable graph answer for dead project files and exports.',
          ),
        ),
        h(
          'div',
          { className: 'doc-card-grid' },
          homeCards.map((card) =>
            h(
              'a',
              { className: 'doc-card', href: card.href, key: card.href },
              h('span', null, card.title),
              h('p', null, card.description),
            ),
          ),
        ),
      ),
    ),
    h(
      'section',
      { className: 'section section-ink' },
      h(
        'div',
        { className: 'section-inner workflow-grid' },
        h(
          'div',
          null,
          h('p', { className: 'eyebrow' }, 'Fast path'),
          h('h2', null, 'Start explicit, then automate'),
          h(
            'p',
            null,
            'Most teams wire Codescythe around a checked-in config, run read-only JSON in CI, and reserve fix mode for explicit cleanup branches.',
          ),
        ),
        h(
          CodeBlock,
          { language: 'json' },
          `{
  "$schema": "./codescythe.schema.json",
  "entry": ["src/index.ts"],
  "project": ["src/**/*.{ts,tsx}"],
  "testFilePatterns": ["**/*.test.*"],
  "ignore": ["src/generated/**"]
}`,
        ),
      ),
    ),
    h(
      'section',
      { className: 'section section-light' },
      h(
        'div',
        { className: 'section-inner benchmark-grid' },
        h(
          'div',
          null,
          h('p', { className: 'eyebrow' }, 'Performance'),
          h('h2', null, 'Measured on real repositories'),
          h(
            'p',
            null,
            'The benchmark harness compares Codescythe and Knip against pinned real-world TypeScript repositories fetched through Bazel.',
          ),
        ),
        BenchmarkPanel(),
      ),
    ),
  );
}

function BenchmarkPanel() {
  const rows = [
    { repo: 'microsoft/vscode', files: '9,398 files', codescythe: '1.11s', knip: '4.22s', ratio: '26%' },
    { repo: 'grafana/grafana', files: '8,358 files', codescythe: '833ms', knip: '9.51s', ratio: '9%' },
    { repo: 'elastic/kibana', files: '90,931 files', codescythe: '13.61s', knip: '43.04s', ratio: '32%' },
    { repo: 'renovatebot/renovate', files: '2,456 files', codescythe: '154ms', knip: '900ms', ratio: '17%' },
  ];
  return h(
    'div',
    { className: 'benchmark-panel', 'aria-label': 'Benchmark comparison' },
    rows.map((row) =>
      h(
        'div',
        { className: 'benchmark-row', key: row.repo },
        h('div', null, h('span', { className: 'fixture' }, row.repo), h('span', { className: 'files' }, row.files)),
        h(
          'div',
          { className: 'bars' },
          h('span', { className: 'bar codescythe', style: { '--value': row.ratio } }, row.codescythe),
          h('span', { className: 'bar knip', style: { '--value': '100%' } }, row.knip),
        ),
      ),
    ),
    h(
      'div',
      { className: 'legend', 'aria-hidden': 'true' },
      h('span', null, h('i', { className: 'legend-codescythe' }), 'Codescythe'),
      h('span', null, h('i', { className: 'legend-knip' }), 'Knip'),
    ),
  );
}

const pages: Page[] = [
  {
    slug: 'getting-started',
    title: 'Getting Started',
    eyebrow: 'First run',
    description: 'Install Codescythe, add a minimal config, run the analyzer, and apply supported fixes.',
    sections: [
      { id: 'install', title: 'Install' },
      { id: 'minimal-config', title: 'Minimal Config' },
      { id: 'run-analysis', title: 'Run Analysis' },
      { id: 'apply-fixes', title: 'Apply Fixes' },
      { id: 'ci', title: 'CI Workflow' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'install', title: 'Install' },
        h(
          'p',
          null,
          'Use the public npm package for local development and CI. The loader selects the matching native package for supported platforms.',
        ),
        h(CodeBlock, null, `npm install -D codescythe`),
      ),
      h(
        PageSection,
        { id: 'minimal-config', title: 'Minimal Config' },
        h(
          'p',
          null,
          'Start by naming the files that make the application reachable and the project files that Codescythe is allowed to report.',
        ),
        h(
          CodeBlock,
          { language: 'json' },
          `{
  "$schema": "./codescythe.schema.json",
  "entry": ["src/index.ts"],
  "project": ["src/**/*.{ts,tsx}"],
  "testFilePatterns": ["**/*.test.*"]
}`,
        ),
        h(
          Callout,
          { title: 'Keep the boundary small at first' },
          h(
            'p',
            null,
            'A narrow project glob is easier to review. Expand the scope after the first read-only report looks credible.',
          ),
        ),
      ),
      h(
        PageSection,
        { id: 'run-analysis', title: 'Run Analysis' },
        h(
          Steps,
          {
            items: [
              {
                title: 'Run read-only JSON',
                body: h(React.Fragment, null, h(CodeBlock, null, `npx codescythe --json --config codescythe.jsonc`)),
              },
              {
                title: 'Inspect unused files and exports',
                body: h('p', null, 'Unused project files and unused exports are reported separately so you can review deletion and export-edit risk independently.'),
              },
              {
                title: 'Rerun with verbose diagnostics when changing config',
                body: h(React.Fragment, null, h(CodeBlock, null, `npx codescythe --verbose --json --config codescythe.jsonc`)),
              },
            ],
          },
        ),
      ),
      h(
        PageSection,
        { id: 'apply-fixes', title: 'Apply Fixes' },
        h(
          'p',
          null,
          'Fix mode removes unused project files and supported export declarations. It refuses risky source-like unresolved-import ignore patterns unless you explicitly force the run.',
        ),
        h(CodeBlock, null, `npx codescythe --fix --config codescythe.jsonc`),
        h(
          Callout,
          { title: 'Run again after a fix pass' },
          h('p', null, 'Removing a dead file can expose another dead file or export. Repeat the analysis when you need a stable final cleanup branch.'),
        ),
      ),
      h(
        PageSection,
        { id: 'ci', title: 'CI Workflow' },
        h('p', null, 'A practical CI lane runs read-only JSON and fails when Codescythe finds issues. Keep destructive fixes in reviewed cleanup branches.'),
        h(CodeBlock, null, `npx codescythe --json --config codescythe.jsonc`),
      ),
    ),
  },
  {
    slug: 'configuration',
    title: 'Configuration',
    eyebrow: 'Source graph',
    description: 'Understand entry files, project globs, test file patterns, ignores, aliases, and unresolved imports.',
    sections: [
      { id: 'discovery', title: 'Config Discovery' },
      { id: 'entry', title: 'Entry Files' },
      { id: 'project', title: 'Project Files' },
      { id: 'tests', title: 'Test File Patterns' },
      { id: 'fields', title: 'Config Fields' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'discovery', title: 'Config Discovery' },
        h('p', null, 'Codescythe reads config from an explicit path, root config file, or package manifest. Explicit paths are best for CI because they remove ambiguity.'),
        h(
          'ul',
          null,
          h('li', null, h('code', null, 'codescythe.json')),
          h('li', null, h('code', null, 'codescythe.jsonc')),
          h('li', null, h('code', null, 'package.json'), ' under the ', h('code', null, 'codescythe'), ' key'),
          h('li', null, 'an explicit ', h('code', null, '--config'), ' path'),
        ),
      ),
      h(
        PageSection,
        { id: 'entry', title: 'Entry Files' },
        h('p', null, 'Entry files are the reachable roots of the source graph. A file or export reachable from an entry is treated as used.'),
        h(CodeBlock, { language: 'json' }, `"entry": [
  "src/index.ts",
  "src/cli.ts",
  "src/routes/**/*.tsx"
]`),
        h('p', null, 'Model detached applications, CLI entrypoints, package exports, and end-to-end specs as entries when they should keep their imports alive.'),
      ),
      h(
        PageSection,
        { id: 'project', title: 'Project Files' },
        h('p', null, 'Project globs define the files Codescythe is allowed to report as unused. Keep generated output, vendored code, build artifacts, and intentionally detached examples outside the project set or under ignore rules.'),
        h(CodeBlock, { language: 'json' }, `"project": [
  "src/**/*.{js,jsx,ts,tsx}",
  "packages/*/src/**/*.{ts,tsx}"
]`),
      ),
      h(
        PageSection,
        { id: 'tests', title: 'Test File Patterns' },
        h(
          'p',
          null,
          'Files matching test file patterns are treated as leaf files. They can be removed when they only test code that is being removed, but they do not make production imports look used.',
        ),
        h(CodeBlock, { language: 'json' }, `"testFilePatterns": [
  "**/*.test.*"
]`),
        h(
          Callout,
          { title: 'Default pattern' },
          h('p', null, 'The default is ', h('code', null, '**/*.test.*'), '. ', h('code', null, '.spec.*'), ' files are ordinary project files unless you configure them or make them entries.'),
        ),
      ),
      h(
        PageSection,
        { id: 'fields', title: 'Config Fields' },
        h(FieldTable, {
          rows: [
            {
              field: 'entry',
              purpose: 'Reachability roots',
              type: 'string or string[]',
              values: 'File paths or glob patterns, relative to the analysis root.',
              notes: 'Files and globs that keep imports and exports alive. Use one entry per application, CLI, package boundary, or detached spec root that should anchor reachability.',
              example: `"entry": [
  "src/index.ts",
  "src/cli.ts",
  "src/routes/**/*.tsx"
]`,
            },
            {
              field: 'project',
              purpose: 'Reportable source files',
              type: 'string or string[]',
              values: 'File paths or glob patterns for source files Codescythe may report.',
              notes: 'Keep generated output, vendored files, build output, and intentional examples outside this set or covered by ignore rules.',
              example: `"project": [
  "src/**/*.{js,jsx,ts,tsx}",
  "packages/*/src/**/*.{ts,tsx}"
]`,
            },
            {
              field: 'testFilePatterns',
              purpose: 'Leaf test classification',
              type: 'string or string[]',
              values: 'Glob patterns. Default: ["**/*.test.*"].',
              notes: 'Matching files do not mark production imports as used. Configure .spec files explicitly if they should behave like test leaves.',
              example: `"testFilePatterns": [
  "**/*.test.*",
  "**/*.spec.*"
]`,
            },
            {
              field: 'ignore',
              purpose: 'Exclude files',
              type: 'string or string[]',
              values: 'Glob patterns matched before analysis.',
              notes: 'Use for generated, vendored, or otherwise intentionally detached files. Doctor warns when generated-looking ignore patterns also match checked source files.',
              example: `"ignore": [
  "src/generated/**",
  "**/*.stories.tsx"
]`,
            },
            {
              field: 'aliases',
              purpose: 'Import resolution',
              type: 'object mapping string keys to string or string[] targets',
              values: 'Keys may use wildcards such as "#app/*"; values are relative target patterns such as "src/*".',
              notes: 'Explicit source alias mappings override or supplement package metadata when package.json imports are not enough.',
              example: `"aliases": {
  "#app/*": "src/*",
  "#generated/*": ["src/generated/*"]
}`,
            },
            {
              field: 'unresolvedImports',
              purpose: 'Resolver policy',
              type: 'object',
              values: 'mode: "report" | "ignore" | "error"; ignore: string or string[]. Default mode is "report".',
              notes: 'Use report while tuning config, error in CI when unresolved source imports are unacceptable, and ignore only for reviewed non-source patterns.',
              example: `"unresolvedImports": {
  "mode": "error",
  "ignore": ["*.svg?raw", "virtual:*"]
}`,
            },
            {
              field: 'includeEntryExports',
              purpose: 'Entry export handling',
              type: 'boolean',
              values: 'Default: false.',
              notes: 'Set true when entry files are also public export surfaces and their exports should be checked instead of automatically preserved.',
              example: `"includeEntryExports": true`,
            },
            {
              field: 'ignoreExportsUsedInFile',
              purpose: 'Local export usage',
              type: 'boolean',
              values: 'Default: false.',
              notes: 'Suppresses exported symbols that are referenced inside their declaring file. Use sparingly for modules with deliberate local export patterns.',
              example: `"ignoreExportsUsedInFile": true`,
            },
          ],
        }),
      ),
    ),
  },
  {
    slug: 'features',
    title: 'Features',
    eyebrow: 'Analyzer surface',
    description: 'Review the analyzer, fix mode, verbose diagnostics, export explanations, and doctor checks.',
    sections: [
      { id: 'graph', title: 'Import Graph' },
      { id: 'query', title: 'Dependency Queries' },
      { id: 'unused', title: 'Unused Files and Exports' },
      { id: 'fix', title: 'Fix Mode' },
      { id: 'explain', title: 'Explanations' },
      { id: 'doctor', title: 'Doctor' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'graph', title: 'Import Graph' },
        h('p', null, 'Codescythe follows static imports, re-exports, string-literal dynamic imports, destructured CommonJS requires, and supported ', h('code', null, 'import.meta.glob'), ' patterns.'),
      ),
      h(
        PageSection,
        { id: 'query', title: 'Dependency Queries' },
        h(
          'p',
          null,
          'Use ',
          h('code', null, 'query somepath'),
          ', ',
          h('code', null, 'query somepaths'),
          ', and ',
          h('code', null, 'query allpaths'),
          ' to inspect dependency routes through that graph. Queries can target files, folders, or exported symbols and can render text, JSON, Mermaid, or SVG output. The ',
          h(InlineLink, { href: '../queries/' }, 'dependency query guide'),
          ' includes fixture-backed Mermaid samples.',
        ),
      ),
      h(
        PageSection,
        { id: 'unused', title: 'Unused Files and Exports' },
        h('p', null, 'The report separates unreachable files from unused exported symbols. This keeps deletion decisions and export-edit decisions reviewable.'),
        h(CodeBlock, { language: 'json' }, `{
  "unusedFiles": ["src/legacy/dead-view.ts"],
  "unusedExports": [
    { "file": "src/constants.ts", "symbol": "oldFlag" }
  ],
  "unresolved": []
}`),
      ),
      h(
        PageSection,
        { id: 'fix', title: 'Fix Mode' },
        h('p', null, 'Fix mode removes unused files before export edits, records changed files, and skips export edits when unresolved import uncertainty could hide real usage.'),
        h(CodeBlock, null, `npx codescythe --fix --json --config codescythe.jsonc`),
      ),
      h(
        PageSection,
        { id: 'explain', title: 'Explanations' },
        h(
          'p',
          null,
          'Use export explanations when reviewing a surprising result. Codescythe reports the requested symbol status, the reason for that decision, which importers counted, and which importers were skipped. The ',
          h(InlineLink, { href: '../reports/#explain-report' }, 'report guide'),
          ' shows how to read each field.',
        ),
        h(CodeBlock, null, `npx codescythe --explain-export src/constants.ts:getServerId`),
      ),
      h(
        PageSection,
        { id: 'doctor', title: 'Doctor' },
        h(
          'p',
          null,
          'Doctor mode checks config risk without editing files. It is the fastest way to diagnose suspicious ignores, empty globs, unresolved imports, and source-alias overlap. Read doctor warnings as config risk signals, then use unresolved import diagnostics to see the alias candidates Codescythe tried.',
        ),
        h(CodeBlock, null, `npx codescythe doctor --config codescythe.jsonc`),
      ),
    ),
  },
  {
    slug: 'queries',
    title: 'Dependency Queries',
    eyebrow: 'Path inspection',
    description: 'Use somepath, somepaths, and allpaths to explain dependency paths through the same graph Codescythe analyzes.',
    sections: [
      { id: 'commands', title: 'Commands' },
      { id: 'selectors', title: 'Selectors' },
      { id: 'formats', title: 'Output Formats' },
      { id: 'fixture-examples', title: 'Fixture Examples' },
      { id: 'cycles', title: 'Cycles' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'commands', title: 'Commands' },
        h(
          'p',
          null,
          'The query command traces dependency paths between two selectors without changing analysis results or editing files. It uses the same parsed import, export, resolver, alias, dynamic import, and glob edges as normal analysis.',
        ),
        h(CodeBlock, null, `npx codescythe query somepath src/main.ts src/module.ts
npx codescythe query somepaths src/main.ts src/features/
npx codescythe query allpaths src/main.ts src/runtime.ts:initRuntime`),
        h(FieldTable, {
          rows: [
            {
              field: 'somepath',
              purpose: 'One shortest path',
              type: 'query verb',
              values: 'source selector, target selector',
              notes: 'Returns the first shortest dependency path found from any matched source node to any matched target node.',
            },
            {
              field: 'somepaths',
              purpose: 'One path per target',
              type: 'query verb',
              values: 'source selector, target selector',
              notes: 'Returns one shortest path for each reachable matched target, which is useful when the target selector is a directory.',
            },
            {
              field: 'allpaths',
              purpose: 'Every path edge as a subgraph',
              type: 'query verb',
              values: 'source selector, target selector',
              notes: 'Returns every node and edge that lies on at least one route from the source selector to the target selector.',
            },
          ],
        }),
      ),
      h(
        PageSection,
        { id: 'selectors', title: 'Selectors' },
        h('p', null, 'Selectors can point at files, directories, or exported symbols. Relative selectors are resolved from the analysis root selected by ', h('code', null, '-C'), ' or ', h('code', null, '--config'), '.'),
        h(
          'ul',
          null,
          h('li', null, h('code', null, 'src/main.ts'), ' selects a project file.'),
          h('li', null, h('code', null, 'src/features/'), ' selects every project file under a directory.'),
          h('li', null, h('code', null, 'src/module.ts:used'), ' selects one exported symbol from a file.'),
        ),
        h(
          Callout,
          { title: 'Export selectors stay symbol-aware' },
          h('p', null, 'A named import path can stop at ', h('code', null, 'file.ts:symbol'), ' before following the export-definition edge into the file node. This keeps file reachability and export usage distinguishable.'),
        ),
      ),
      h(
        PageSection,
        { id: 'formats', title: 'Output Formats' },
        h('p', null, 'Text output is optimized for terminal inspection. JSON is the stable machine-readable surface. Mermaid and SVG render the same query graph as a diagram.'),
        h(CodeBlock, null, `npx codescythe query allpaths src/main.ts src/runtime.ts:initRuntime --output text
npx codescythe query allpaths src/main.ts src/runtime.ts:initRuntime --json
npx codescythe query allpaths src/main.ts src/runtime.ts:initRuntime --output mermaid
npx codescythe query allpaths src/main.ts src/runtime.ts:initRuntime --output svg > graph.svg`),
        h(
          'p',
          null,
          h('code', null, '--json'),
          ' is a shortcut for ',
          h('code', null, '--output json'),
          '. SVG output is rendered from the Mermaid graph with ',
          h('code', null, 'mermaid-rs-renderer'),
          '.',
        ),
      ),
      h(
        PageSection,
        { id: 'fixture-examples', title: 'Fixture Examples' },
        h(
          'p',
          null,
          'These examples are generated from checked-in repository fixtures. The Mermaid snippets below are exact CLI output from ',
          h('code', null, '--output mermaid'),
          '.',
        ),
        h(
          'div',
          { className: 'query-samples' },
          h(QueryFixtureSample, {
            title: 'test-file-usage: somepath to one export',
            description: 'A file-to-export query shows a named import edge directly to the exported symbol.',
            command: 'codescythe query somepath -C tests/fixtures/test-file-usage --output mermaid src/main.ts src/module.ts:used',
            mermaid: `flowchart LR
  n0["src/module.ts:used"]
  n1["src/main.ts"]
  n1 -->|"named import ./module:used"| n0`,
          }),
          h(QueryFixtureSample, {
            title: 'oxc-resolution: somepaths to a folder',
            description: 'A file-to-directory query returns one shortest path for each reachable matched target file.',
            command: 'codescythe query somepaths -C tests/fixtures/oxc-resolution --output mermaid app/index.ts app/',
            mermaid: `flowchart LR
  n0["app/aliased.ts:aliased"]
  n1["app/extension.ts:extension"]
  n2["app/internal.ts:internal"]
  n3["app/aliased.ts"]
  n4["app/extension.ts"]
  n5["app/index.ts"]
  n6["app/internal.ts"]
  n0 -->|"defined in file aliased"| n3
  n1 -->|"defined in file extension"| n4
  n2 -->|"defined in file internal"| n6
  n5 -->|"named import @/aliased:aliased"| n0
  n5 -->|"named import ./extension.js:extension"| n1
  n5 -->|"named import #internal:internal"| n2`,
          }),
          h(QueryFixtureSample, {
            title: 'knip-export-basics: allpaths through namespace use',
            description: 'An allpaths query keeps every node and edge that can carry the source file to the target export.',
            command: 'codescythe query allpaths -C tests/fixtures/knip-export-basics --output mermaid index.ts my-namespace.ts:y',
            mermaid: `flowchart LR
  n0["index.ts"]
  n1["my-module.ts"]
  n2["my-module.ts:myExport"]
  n3["my-namespace.ts:y"]
  n2 -->|"defined in file myExport"| n1
  n0 -->|"named import ./my-module.js:myExport"| n2
  n1 -->|"namespace member ./my-namespace.js:y"| n3`,
          }),
          h(QueryFixtureSample, {
            title: 'runfiles-fixture: somepath through an alias',
            description: 'Alias resolution is represented in the edge label, while the target still resolves to the project file export.',
            command: 'codescythe query somepath -C tests/fixtures/runfiles-fixture --output mermaid workspace/frontend/apps/client/platform/platformRuntime.ts protobuf/generated/client.ts:client',
            mermaid: `flowchart LR
  n0["protobuf/generated/client.ts:client"]
  n1["workspace/frontend/apps/client/platform/platformRuntime.ts"]
  n1 -->|"named import #bazel_generated/client:client"| n0`,
          }),
        ),
      ),
      h(
        PageSection,
        { id: 'cycles', title: 'Cycles' },
        h(
          'p',
          null,
          'Dependency cycles are finite in query output. ',
          h('code', null, 'somepath'),
          ' and ',
          h('code', null, 'somepaths'),
          ' run breadth-first search with visited nodes and parent edges, so they return shortest acyclic paths. ',
          h('code', null, 'allpaths'),
          ' does not enumerate paths; it intersects forward reachability from the source with reverse reachability from the target, then returns the induced path subgraph.',
        ),
      ),
    ),
  },
  {
    slug: 'reports',
    title: 'Reports',
    eyebrow: 'Reading output',
    description: 'Understand analyzer JSON, doctor warnings, unresolved import diagnostics, and explain-export reports.',
    sections: [
      { id: 'analysis-json', title: 'Analysis JSON' },
      { id: 'doctor-output', title: 'Doctor Output' },
      { id: 'doctor-warning-codes', title: 'Doctor Warning Codes' },
      { id: 'unresolved-diagnostics', title: 'Unresolved Diagnostics' },
      { id: 'explain-report', title: 'Explain Export Report' },
      { id: 'query-output', title: 'Query Output' },
      { id: 'review-workflow', title: 'Review Workflow' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'analysis-json', title: 'Analysis JSON' },
        h('p', null, 'The normal JSON report is the compact machine-readable surface for automation. Use it to gate CI or drive cleanup review, then switch to verbose JSON only when you need config and discovery diagnostics.'),
        h(CodeBlock, { language: 'json' }, `{
  "issues": {
    "files": { "src/legacy/dead-view.ts": {} },
    "exports": {
      "src/constants.ts": {
        "oldFlag": { "symbol": "oldFlag", "line": 4, "col": 14 }
      }
    },
    "unresolved": {}
  },
  "counters": {
    "files": 1,
    "exports": 1,
    "unresolved": 0,
    "processed": 42,
    "total": 42
  }
}`),
        h(
          Callout,
          { title: 'Verbose mode is for diagnosis' },
          h('p', null, h('code', null, '--verbose --json'), ' adds resolved config, matched glob counts, ignored unresolved import samples, and unused export explanations. Keep non-verbose JSON for stable automation.'),
        ),
      ),
      h(
        PageSection,
        { id: 'doctor-output', title: 'Doctor Output' },
        h('p', null, 'Doctor returns a summary, sorted warning list, and sampled unresolved import diagnostics. It exits with findings when warnings or unresolved diagnostics are present, but it never edits files.'),
        h(CodeBlock, { language: 'json' }, `{
  "warnings": [
    {
      "code": "entryGlobZeroMatches",
      "message": "entry pattern \\"src/app/**/*.tsx\\" matched no project files"
    }
  ],
  "summary": {
    "version": "0.4.13",
    "configPath": "codescythe.jsonc",
    "projectCount": 184,
    "entryCount": 2,
    "ignoredUnresolvedCount": 0,
    "ignoredUnresolvedPatterns": ["*.svg?raw"],
    "packageImportKeys": ["#app/*"],
    "configuredAliasKeys": ["#generated/*"]
  }
}`),
        h(FieldTable, {
          rows: [
            { field: 'warnings', purpose: 'Config risk list', type: 'ConfigDoctorWarning[]', values: 'Each item has code and message.', notes: 'Treat every warning as a config review item before widening scope or forcing a fix.' },
            { field: 'summary.projectCount', purpose: 'Project file count', type: 'number', values: 'Count after project, ignore, and gitignore filtering.', notes: 'A very high count with a tiny entry count often means project is too broad for the current entries.' },
            { field: 'summary.entryCount', purpose: 'Matched entry count', type: 'number', values: 'Count of entry files that matched project files.', notes: 'Zero entries means analysis cannot establish reachability from the configured roots.' },
            { field: 'summary.ignoredUnresolvedPatterns', purpose: 'Ignored resolver patterns', type: 'string[]', values: 'Patterns from unresolvedImports.ignore.', notes: 'Broad JS/TS-family source patterns should be reviewed with extra care because they can hide real usage.' },
            { field: 'summary.packageImportKeys', purpose: 'Package import aliases', type: 'string[]', values: 'Keys discovered from package metadata.', notes: 'Useful when source alias overlap makes unresolved ignore rules risky.' },
            { field: 'summary.configuredAliasKeys', purpose: 'Config aliases', type: 'string[]', values: 'Keys from aliases in Codescythe config.', notes: 'If an ignored unresolved import overlaps one of these, resolve before ignore.' },
          ],
        }),
      ),
      h(
        PageSection,
        { id: 'doctor-warning-codes', title: 'Doctor Warning Codes' },
        h(FieldTable, {
          rows: [
            { field: 'entryGlobZeroMatches', purpose: 'Entry matched nothing', type: 'warning code', values: 'Emitted per entry pattern.', notes: 'Fix the path, widen project, or remove the stale entry. A zero-match entry cannot keep anything reachable.' },
            { field: 'unresolvedImports', purpose: 'Analysis has unresolved imports', type: 'warning code', values: 'Emitted when analysis reported unresolved import edges.', notes: 'Use the unresolved diagnostics section to inspect resolver errors and alias candidate files.' },
            { field: 'sourceAliasUnresolvedIgnore', purpose: 'Ignore overlaps source alias', type: 'warning code', values: 'Emitted for unresolvedImports.ignore patterns that may match local aliases.', notes: 'This is the main resolve-before-ignore warning. Fix mode can refuse risky source-like patterns unless forced.' },
            { field: 'projectScopeMuchBroaderThanEntryCoverage', purpose: 'Project likely too broad', type: 'warning code', values: 'Emitted when most project files are unused from current entries.', notes: 'Add missing entries or narrow project before trusting a large deletion report.' },
            { field: 'ignoredGeneratedPatternMatchesSource', purpose: 'Generated ignore catches source', type: 'warning code', values: 'Emitted when an ignore pattern containing generated also matches checked source.', notes: 'Narrow generated ignores so they do not mask hand-written code.' },
          ],
        }),
      ),
      h(
        PageSection,
        { id: 'unresolved-diagnostics', title: 'Unresolved Diagnostics' },
        h('p', null, 'Doctor samples unresolved imports and asks the resolver to explain what it tried. Use this section to decide whether an import is a real missing file, an alias mapping gap, or a safe non-source asset.'),
        h(CodeBlock, { language: 'json' }, `{
  "unresolvedImports": [
    {
      "importer": "src/routes/home.tsx",
      "specifier": "#generated/client",
      "resolverError": "module not found",
      "matchedAliases": [
        {
          "source": "config",
          "key": "#generated/*",
          "target": "src/generated/*",
          "expandedTarget": "src/generated/client",
          "candidateFiles": [
            {
              "path": "src/generated/client.ts",
              "exists": false,
              "inProject": false
            }
          ]
        }
      ]
    }
  ]
}`),
        h(
          'ul',
          null,
          h('li', null, h('code', null, 'importer'), ' is the file that contains the unresolved import.'),
          h('li', null, h('code', null, 'specifier'), ' is the raw import string.'),
          h('li', null, h('code', null, 'matchedAliases'), ' shows package or config aliases that looked relevant.'),
          h('li', null, h('code', null, 'candidateFiles'), ' shows resolver candidates and whether each exists or is inside project scope.'),
        ),
      ),
      h(
        PageSection,
        { id: 'explain-report', title: 'Explain Export Report' },
        h('p', null, 'Use ', h('code', null, '--explain-export <file>:<symbol>'), ' when an export result is surprising. In text mode, Codescythe prints a human-readable explanation. With JSON, the explanation is available under ', h('code', null, 'explainExport'), '.'),
        h(CodeBlock, null, `npx codescythe --explain-export src/constants.ts:oldFlag
npx codescythe --json --explain-export src/constants.ts:oldFlag`),
        h(CodeBlock, { language: 'json' }, `{
  "explainExport": {
    "exportingFile": "src/constants.ts",
    "symbol": "oldFlag",
    "status": "dead",
    "reason": "export is not used by reachable importers",
    "explanation": {
      "fileReachable": true,
      "importersConsidered": [],
      "importersSkipped": [
        {
          "importer": "src/constants.test.ts",
          "specifier": "./constants",
          "reason": "test file leaf"
        }
      ],
      "ignoredUnresolvedImportsThatMightHavePointedAtThisFile": []
    }
  }
}`),
        h(FieldTable, {
          rows: [
            { field: 'status', purpose: 'Decision', type: 'enum', values: 'alive | dead | fileUnused | fileNotFound | symbolNotExported', notes: 'Alive exports are kept; dead exports can be removed if no safety guard blocks the edit.' },
            { field: 'reason', purpose: 'Short explanation', type: 'string', values: 'Generated from the graph decision.', notes: 'Read this first, then inspect importer details when it disagrees with expectation.' },
            { field: 'fileReachable', purpose: 'Reachability check', type: 'boolean', values: 'true when the exporting file is reachable from configured entries.', notes: 'If false, the export is usually secondary to an unused-file finding.' },
            { field: 'importersConsidered', purpose: 'Importers that count', type: 'ExportImportExplanation[]', values: 'Each item has importer, specifier, and reason.', notes: 'Reasons include named import, namespace member access, re-export, dynamic import marks all exports, and export star marks all exports.' },
            { field: 'importersSkipped', purpose: 'Importers that do not count', type: 'SkippedImporterExplanation[]', values: 'Each item has importer, specifier, and reason.', notes: 'Common reasons are test file leaf or importer unreachable.' },
            { field: 'ignoredUnresolvedImportsThatMightHavePointedAtThisFile', purpose: 'Uncertainty from ignored imports', type: 'IgnoredUnresolvedImportSample[]', values: 'Samples with specifier and importer.', notes: 'If this is non-empty, review unresolvedImports.ignore before trusting an export edit.' },
          ],
        }),
      ),
      h(
        PageSection,
        { id: 'query-output', title: 'Query Output' },
        h(
          'p',
          null,
          'Query JSON includes the parsed selectors, matched source and target nodes, unresolved imports observed while building the graph, and either paths or a graph depending on the query kind.',
        ),
        h(CodeBlock, { language: 'json' }, `{
  "kind": "somepath",
  "from": { "kind": "file", "path": "src/main.ts" },
  "to": { "kind": "export", "path": "src/module.ts", "symbol": "used" },
  "paths": [
    {
      "nodes": [
        { "id": "file:src/main.ts", "kind": "file", "path": "src/main.ts" },
        { "id": "export:src/module.ts:used", "kind": "export", "path": "src/module.ts", "symbol": "used" }
      ],
      "edges": [
        { "kind": "namedImport", "specifier": "./module", "imported": "used" }
      ]
    }
  ]
}`),
        h(
          'p',
          null,
          h('code', null, 'allpaths'),
          ' returns ',
          h('code', null, 'graph.nodes'),
          ' and ',
          h('code', null, 'graph.edges'),
          ' instead of ',
          h('code', null, 'paths'),
          '. Diagram formats render that same data as Mermaid or SVG.',
        ),
      ),
      h(
        PageSection,
        { id: 'review-workflow', title: 'Review Workflow' },
        h(
          Steps,
          {
            items: [
              {
                title: 'Start with doctor',
                body: h(React.Fragment, null, h(CodeBlock, null, `npx codescythe doctor --json --config codescythe.jsonc`)),
              },
              {
                title: 'Fix config-risk warnings first',
                body: h('p', null, 'Zero-match entries, broad project scopes, and source-alias unresolved ignores can all make the main cleanup report less trustworthy.'),
              },
              {
                title: 'Explain surprising exports',
                body: h(React.Fragment, null, h(CodeBlock, null, `npx codescythe --json --explain-export src/module.ts:legacyExport`)),
              },
              {
                title: 'Only then run fix mode',
                body: h('p', null, 'A clean doctor report and explain output that matches your expectations give fix mode a much narrower review surface.'),
              },
            ],
          },
        ),
      ),
    ),
  },
  {
    slug: 'troubleshooting',
    title: 'Troubleshooting',
    eyebrow: 'Debugging runs',
    description: 'Use doctor and verbose diagnostics to understand empty scans, unresolved imports, and risky ignores.',
    sections: [
      { id: 'doctor', title: 'Start with Doctor' },
      { id: 'empty', title: 'Empty or Tiny Results' },
      { id: 'unresolved', title: 'Unresolved Imports' },
      { id: 'fix-refused', title: 'Fix Refused' },
      { id: 'surprising-live', title: 'Surprising Live Code' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'doctor', title: 'Start with Doctor' },
        h(
          'p',
          null,
          'Doctor is built for config triage. Run it before widening project scope or forcing a fix, then read the warning code first and the message second. The code tells you what class of risk Codescythe found; the message names the concrete pattern, count, or file involved.',
        ),
        h(CodeBlock, null, `npx codescythe doctor --config codescythe.jsonc
npx codescythe doctor --json --config codescythe.jsonc`),
        h('p', null, 'Use the ', h(InlineLink, { href: '../reports/#doctor-output' }, 'doctor output guide'), ' when JSON includes unresolved import diagnostics or alias candidates.'),
      ),
      h(
        PageSection,
        { id: 'empty', title: 'Empty or Tiny Results' },
        h('p', null, 'If the report is unexpectedly empty, verify that entry and project globs match real files from the same directory root. Verbose mode prints matched entry and project counts.'),
        h(CodeBlock, null, `npx codescythe --verbose --json --config codescythe.jsonc`),
      ),
      h(
        PageSection,
        { id: 'unresolved', title: 'Unresolved Imports' },
        h(
          'p',
          null,
          'Unresolved imports can make cleanup unsafe because an unresolved source import may hide real usage. Inspect resolver diagnostics before ignoring broad patterns: candidate files with ',
          h('code', null, 'exists=false'),
          ' usually point to missing generated output or a bad alias target; candidates with ',
          h('code', null, 'exists=true'),
          ' but ',
          h('code', null, 'inProject=false'),
          ' usually mean project scope is too narrow.',
        ),
        h(CodeBlock, { language: 'json' }, `"unresolvedImports": {
  "mode": "error",
  "ignore": ["*.svg?raw"]
}`),
      ),
      h(
        PageSection,
        { id: 'fix-refused', title: 'Fix Refused' },
        h('p', null, 'Fix mode refuses source-like unresolved-import ignore patterns that overlap package imports or configured aliases. Narrow the pattern, fix the resolver issue, or force only after reviewing doctor output.'),
        h(Callout, { title: 'Force is a last step' }, h('p', null, h('code', null, '--force'), ' bypasses a safety guard. Keep the run scoped and review the diff.')),
      ),
      h(
        PageSection,
        { id: 'surprising-live', title: 'Surprising Live Code' },
        h(
          'p',
          null,
          'When a symbol stays live unexpectedly, use export explanations and inspect whether a reachable importer, entry export rule, barrel export, dynamic import, or ignored unresolved import is keeping it reachable.',
        ),
        h(CodeBlock, null, `npx codescythe --explain-export src/module.ts:legacyExport`),
        h('p', null, 'If JSON reports ', h('code', null, 'importersSkipped'), ', those imports were observed but did not count because the importer was unreachable or treated as a test leaf. If ', h('code', null, 'ignoredUnresolvedImportsThatMightHavePointedAtThisFile'), ' is non-empty, resolve that uncertainty before editing exports.'),
      ),
    ),
  },
  {
    slug: 'performance',
    title: 'Performance',
    eyebrow: 'Large repositories',
    description: 'Understand the benchmark story, analysis model, and tuning options for large TypeScript codebases.',
    sections: [
      { id: 'benchmarks', title: 'Benchmarks' },
      { id: 'model', title: 'Analysis Model' },
      { id: 'tuning', title: 'Tuning' },
      { id: 'repeat', title: 'Repeatable Runs' },
    ],
    body: h(
      React.Fragment,
      null,
      h(
        PageSection,
        { id: 'benchmarks', title: 'Benchmarks' },
        h('p', null, 'Representative local runs against pinned fixtures show Codescythe focusing on files and exports with a smaller runtime profile than broader project-audit tooling.'),
        BenchmarkPanel(),
        h('p', null, 'Run the same harness locally with ', h('code', null, 'pnpm benchmark'), ' or target one fixture with ', h('code', null, 'pnpm benchmark:kibana'), '.'),
      ),
      h(
        PageSection,
        { id: 'model', title: 'Analysis Model' },
        h('p', null, 'Codescythe discovers the project file set, parses files in parallel graph-frontier batches from configured entries, and avoids framework-plugin discovery.'),
      ),
      h(
        PageSection,
        { id: 'tuning', title: 'Tuning' },
        h('p', null, 'Use ', h('code', null, 'CODESCYTHE_PARSE_THREADS'), ' to tune parse parallelism. ', h('code', null, 'RAYON_NUM_THREADS'), ' is respected when the Codescythe-specific variable is unset.'),
        h(CodeBlock, null, `CODESCYTHE_PARSE_THREADS=8 npx codescythe --json --config codescythe.jsonc`),
      ),
      h(
        PageSection,
        { id: 'repeat', title: 'Repeatable Runs' },
        h('p', null, 'For stable cleanup automation, keep config checked in, run the same command locally and in CI, and rerun after fix mode because deletions can expose additional unused code.'),
      ),
    ),
  },
];

function DocPage({ page }: { page: Page }) {
  return h(
    'main',
    { className: 'doc-layout', id: 'top' },
    h(
      'aside',
      { className: 'doc-sidebar', 'aria-label': 'Documentation navigation' },
      h('a', { className: 'sidebar-home', href: '../' }, 'Overview'),
      h(
        'nav',
        null,
        pages.map((item) =>
          h(
            'a',
            {
              href: item.slug === page.slug ? './' : `../${item.slug}/`,
              className: item.slug === page.slug ? 'active' : undefined,
              key: item.slug,
            },
            item.title,
          ),
        ),
      ),
    ),
    h(
      'article',
      { className: 'doc-content' },
      h('p', { className: 'eyebrow' }, page.eyebrow),
      h('h1', null, page.title),
      h('p', { className: 'doc-description' }, page.description),
      page.body,
    ),
    page.sections.length > 0 &&
      h(
        'aside',
        { className: 'page-toc', 'aria-label': 'On this page' },
        h('span', null, 'On this page'),
        page.sections.map((section) => h('a', { href: `#${section.id}`, key: section.id }, section.title)),
      ),
  );
}

function SiteShell({
  title,
  description,
  relativePrefix,
  children,
}: {
  title: string;
  description: string;
  relativePrefix: string;
  children: React.ReactNode;
}) {
  const rootHref = `${relativePrefix}./`;
  const assetPrefix = `${relativePrefix}assets/`;
  return h(
    'html',
    { lang: 'en' },
    h(
      'head',
      null,
      h('meta', { charSet: 'utf-8' }),
      h('meta', { name: 'viewport', content: 'width=device-width, initial-scale=1' }),
      h('title', null, title === 'Codescythe' ? 'Codescythe' : `${title} - Codescythe`),
      h('meta', { name: 'description', content: description }),
      h('meta', { property: 'og:title', content: title }),
      h('meta', { property: 'og:description', content: description }),
      h('meta', { property: 'og:type', content: 'website' }),
      h('meta', { name: 'theme-color', content: '#101113' }),
      h('link', { rel: 'icon', type: 'image/png', href: `${assetPrefix}codescythe-logo.png` }),
      h('link', { rel: 'stylesheet', href: `${relativePrefix}vendor/open-props.min.css` }),
      h('link', { rel: 'stylesheet', href: `${relativePrefix}vendor/normalize.dark.min.css` }),
      h('link', { rel: 'stylesheet', href: `${relativePrefix}styles.css` }),
    ),
    h(
      'body',
      null,
      h(
        'header',
        { className: 'site-header', 'aria-label': 'Primary navigation' },
        h(
          'a',
          { className: 'brand', href: rootHref, 'aria-label': 'Codescythe docs home' },
          h('img', { className: 'brand-mark', src: `${assetPrefix}codescythe-logo.png`, alt: '', 'aria-hidden': 'true' }),
          h('span', null, 'Codescythe'),
        ),
        h(
          'nav',
          null,
          primaryNav.map((item) =>
            h('a', { href: `${relativePrefix}${item.href.replace('./', '')}`, key: item.href }, item.label),
          ),
          h('a', { href: 'https://github.com/perplexityai/codescythe' }, 'GitHub'),
        ),
      ),
      children,
      h(
        'footer',
        { className: 'site-footer' },
        h('span', null, 'Codescythe'),
        h('a', { href: 'https://github.com/perplexityai/codescythe' }, 'github.com/perplexityai/codescythe'),
      ),
    ),
  );
}

function renderDocument(element: React.ReactElement) {
  return `<!doctype html>\n${renderToStaticMarkup(element)}\n`;
}

function writePage(filePath: string, element: React.ReactElement) {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, renderDocument(element));
}

function parseBuildArgs(argv = process.argv.slice(2)): BuildOptions {
  const options: BuildOptions = {};

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--out-dir') {
      options.outDir = argv[index + 1];
      index += 1;
    } else if (arg === '--root-dir') {
      options.rootDir = argv[index + 1];
      index += 1;
    } else if (arg === '--quiet') {
      options.quiet = true;
    }
  }

  return options;
}

function build(options: BuildOptions = {}): BuildPaths {
  const { assetDir, publicDir, rootDir, srcDir, vendorDir } = resolveBuildPaths(options);
  const openPropsRoot = path.dirname(require.resolve('open-props/package.json'));

  if (existsSync(publicDir)) {
    rmSync(publicDir, { recursive: true, force: true });
  }
  mkdirSync(assetDir, { recursive: true });
  mkdirSync(vendorDir, { recursive: true });

  copyFileSync(path.join(srcDir, 'site.css'), path.join(publicDir, 'styles.css'));
  copyFileSync(path.join(srcDir, 'assets', 'codescythe-logo.png'), path.join(assetDir, 'codescythe-logo.png'));
  copyFileSync(path.join(openPropsRoot, 'open-props.min.css'), path.join(vendorDir, 'open-props.min.css'));
  copyFileSync(path.join(openPropsRoot, 'normalize.dark.min.css'), path.join(vendorDir, 'normalize.dark.min.css'));
  writeFileSync(path.join(publicDir, '.nojekyll'), '# Static GitHub Pages site for Codescythe.\n');

  writePage(
    path.join(publicDir, 'index.html'),
    h(
      SiteShell,
      {
        title: 'Codescythe',
        description: 'Codescythe is a fast, deterministic dead-code analyzer and remover for TypeScript and JavaScript codebases.',
        relativePrefix: '',
      },
      HomePage(),
    ),
  );

  for (const page of pages) {
    writePage(
      path.join(publicDir, page.slug, 'index.html'),
      h(
        SiteShell,
        {
          title: page.title,
          description: page.description,
          relativePrefix: '../',
        },
        h(DocPage, { page }),
      ),
    );
  }

  if (!options.quiet) {
    console.log(`Built Codescythe docs at ${path.relative(rootDir, publicDir) || publicDir}`);
  }

  return resolveBuildPaths(options);
}

if (require.main === module) {
  build(parseBuildArgs());
}

module.exports = {
  build,
  resolveBuildPaths,
  workspaceRoot,
};

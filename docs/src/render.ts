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

  return { assetDir, docsDir, publicDir, rootDir, srcDir };
}

const primaryNav: NavItem[] = [
  { href: './getting-started/', label: 'Getting Started' },
  { href: './configuration/', label: 'Configuration' },
  { href: './features/', label: 'Features' },
  { href: './troubleshooting/', label: 'Troubleshooting' },
  { href: './performance/', label: 'Performance' },
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

function FieldTable({
  rows,
}: {
  rows: Array<{ field: string; purpose: string; notes: string }>;
}) {
  return h(
    'div',
    { className: 'field-table', role: 'table', 'aria-label': 'Configuration fields' },
    rows.map((row) =>
      h(
        'div',
        { className: 'field-row', role: 'row', key: row.field },
        h('div', { className: 'field-name', role: 'cell' }, h('code', null, row.field)),
        h('div', { role: 'cell' }, h('strong', null, row.purpose), h('p', null, row.notes)),
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
            { field: 'entry', purpose: 'Reachability roots', notes: 'Files and globs that keep imports and exports alive.' },
            { field: 'project', purpose: 'Reportable source files', notes: 'Files Codescythe can flag as unused project files.' },
            { field: 'testFilePatterns', purpose: 'Leaf test classification', notes: 'Patterns for tests that should not mark production source as used.' },
            { field: 'ignore', purpose: 'Exclude files', notes: 'Generated, vendored, or otherwise intentionally detached files.' },
            { field: 'aliases', purpose: 'Import resolution', notes: 'Explicit source alias mappings when package metadata is not enough.' },
            { field: 'unresolvedImports', purpose: 'Resolver policy', notes: 'Control when unresolved imports warn, fail, or are ignored by pattern.' },
            { field: 'includeEntryExports', purpose: 'Entry export handling', notes: 'Preserve exports from entry files when they are public package boundaries.' },
            { field: 'ignoreExportsUsedInFile', purpose: 'Local export usage', notes: 'Suppress per-file export checks for known patterns.' },
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
        h('p', null, 'Use export explanations when reviewing a surprising result. Codescythe will explain why one symbol is dead or alive from the graph it built.'),
        h(CodeBlock, null, `npx codescythe --explain-export src/constants.ts:getServerId`),
      ),
      h(
        PageSection,
        { id: 'doctor', title: 'Doctor' },
        h('p', null, 'Doctor mode checks config risk without editing files. It is the fastest way to diagnose suspicious ignores, empty globs, unresolved imports, and source-alias overlap.'),
        h(CodeBlock, null, `npx codescythe doctor --config codescythe.jsonc`),
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
        h('p', null, 'Doctor is built for config triage. Run it before widening project scope or forcing a fix.'),
        h(CodeBlock, null, `npx codescythe doctor --config codescythe.jsonc
npx codescythe doctor --json --config codescythe.jsonc`),
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
        h('p', null, 'Unresolved imports can make cleanup unsafe because an unresolved source import may hide real usage. Inspect resolver diagnostics before ignoring broad patterns.'),
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
        h('p', null, 'When a symbol stays live unexpectedly, use export explanations and inspect whether a test, entry glob, barrel export, or dynamic import is keeping it reachable.'),
        h(CodeBlock, null, `npx codescythe --explain-export src/module.ts:legacyExport`),
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
      page.sections.length > 0 &&
        h(
          'div',
          { className: 'section-nav' },
          h('span', null, 'On this page'),
          page.sections.map((section) => h('a', { href: `#${section.id}`, key: section.id }, section.title)),
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
      h('meta', { name: 'theme-color', content: '#111827' }),
      h('link', { rel: 'icon', type: 'image/png', href: `${assetPrefix}codescythe-logo.png` }),
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
  const { assetDir, publicDir, rootDir, srcDir } = resolveBuildPaths(options);

  if (existsSync(publicDir)) {
    rmSync(publicDir, { recursive: true, force: true });
  }
  mkdirSync(assetDir, { recursive: true });

  copyFileSync(path.join(srcDir, 'site.css'), path.join(publicDir, 'styles.css'));
  copyFileSync(path.join(srcDir, 'assets', 'codescythe-logo.png'), path.join(assetDir, 'codescythe-logo.png'));
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

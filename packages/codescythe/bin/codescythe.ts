#!/usr/bin/env -S node --experimental-transform-types
'use strict';

type CliOptions = {
  config?: string;
  cwd?: string;
  fix?: boolean;
  json?: boolean;
  verbose?: boolean;
};

type AnalysisDiagnostics = {
  runtime?: {
    version: string;
    processCwd: string;
    resolvedDirectory: string;
    configSource: {
      kind: string;
      path?: string;
    };
    fix: boolean;
    json: boolean;
    verbose: boolean;
  };
  config: {
    entry: string[];
    project: string[];
    ignore: string[];
    testFilePatterns: string[];
    unresolvedImports: {
      mode: string;
      ignore: string[];
    };
    aliases: {
      configured: Record<string, string[]>;
      packageJsonImports: {
        path?: string;
        keys: string[];
      };
    };
  };
  fileDiscovery: {
    projectMatched: number;
    selectedProjectFiles: number;
    ignoredByGitignore: number;
    ignoredByConfig: number;
    parsed: number;
    skippedByExtensionOrType: number;
    entries: number;
    testLeafFiles: number;
  };
  entry: {
    zeroMatchPatterns: string[];
    entryMatchesByPattern: Record<string, string[]>;
    entryFiles: string[];
  };
  deadFiles: Record<
    string,
    {
      matchedEntry: boolean;
      matchedTestFilePatterns: boolean;
      imported: boolean;
      importedBy: string[];
      onlyImportedByDeadOrTestFiles: boolean;
      skippedFromReachabilityDueToTestLeafSemantics: boolean;
      reason: string;
    }
  >;
  fixPlan?: {
    filesToDelete: string[];
    filesWithExportEdits: Record<string, string[]>;
    skippedExportsInDeletedFiles: Record<string, string[]>;
  };
};

type AnalysisResult = {
  counters: {
    exports: number;
    files: number;
    unresolved: number;
  };
  issues: {
    exports: Record<string, Record<string, {col: number; line: number; symbol: string}>>;
    files: Record<string, unknown>;
    unresolved?: Record<string, string[]>;
  };
  diagnostics?: AnalysisDiagnostics;
};

type FixResult = {
  changedFiles: string[];
  removedFiles: string[];
  removedExports: number;
  analysis: AnalysisResult;
};

const { analyze, fix } = require('@perplexity/codescythe') as {
  analyze(options: CliOptions): AnalysisResult;
  fix(options: CliOptions): FixResult;
};

const args = process.argv.slice(2);
const options: CliOptions = {};
let json = false;
let shouldFix = false;
let verbose = false;

for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--json') {
    json = true;
  } else if (arg === '--fix') {
    shouldFix = true;
  } else if (arg === '--verbose') {
    verbose = true;
  } else if (arg === '--config' || arg === '-c') {
    options.config = args[++i];
  } else if (arg === '--directory' || arg === '-C') {
    options.cwd = args[++i];
  } else {
    console.error(`Unknown argument: ${arg}`);
    process.exit(2);
  }
}

options.fix = shouldFix;
options.json = json;
options.verbose = verbose;

const result = shouldFix ? fix(options) : analyze(options);
if (json) {
  console.log(JSON.stringify(result));
} else if (shouldFix) {
  const fixResult = result as FixResult;
  if (verbose) {
    printDiagnostics(fixResult.analysis);
  }
  if (hasIssues(fixResult.analysis)) {
    printTextReport(fixResult.analysis);
    console.log('');
  }
  console.log(
    `Removed ${fixResult.removedExports} unused exports from ${fixResult.changedFiles.length} files and ${fixResult.removedFiles.length} unused files`,
  );
} else {
  const analysisResult = result as AnalysisResult;
  if (verbose) {
    printDiagnostics(analysisResult);
  }
  printTextReport(analysisResult);
}

if (shouldFix) {
  const fixResult = result as FixResult;
  process.exit(hasIssues(fixResult.analysis) ? 1 : 0);
}

const analysisResult = result as AnalysisResult;
process.exit(hasIssues(analysisResult) ? 1 : 0);

function printDiagnostics(analysis: AnalysisResult): void {
  const diagnostics = analysis.diagnostics;
  if (!diagnostics) {
    return;
  }

  console.error('Codescythe diagnostics');
  if (diagnostics.runtime) {
    const runtime = diagnostics.runtime;
    console.error('Runtime:');
    console.error(`  version: ${runtime.version}`);
    console.error(`  process cwd: ${runtime.processCwd}`);
    console.error(`  resolved directory: ${runtime.resolvedDirectory}`);
    if (runtime.configSource.path) {
      console.error(`  config: ${runtime.configSource.path} (${runtime.configSource.kind})`);
    } else {
      console.error(`  config: <default> (${runtime.configSource.kind})`);
    }
    console.error(`  flags: fix=${runtime.fix} json=${runtime.json} verbose=${runtime.verbose}`);
  }

  console.error('Config:');
  console.error(`  entry: ${diagnostics.config.entry.join(', ')}`);
  console.error(`  project: ${diagnostics.config.project.join(', ')}`);
  console.error(`  ignore: ${diagnostics.config.ignore.join(', ')}`);
  console.error(`  testFilePatterns: ${diagnostics.config.testFilePatterns.join(', ')}`);
  console.error(
    `  unresolvedImports: mode=${formatMode(diagnostics.config.unresolvedImports.mode)} ignore=${diagnostics.config.unresolvedImports.ignore.join(', ')}`,
  );
  const configuredAliases = Object.keys(diagnostics.config.aliases.configured);
  if (configuredAliases.length > 0) {
    console.error(`  configured aliases: ${configuredAliases.join(', ')}`);
  }
  const packageImports = diagnostics.config.aliases.packageJsonImports;
  if (packageImports.path) {
    console.error(`  package.json#imports: ${packageImports.path} (${packageImports.keys.join(', ')})`);
  }

  const discovery = diagnostics.fileDiscovery;
  console.error('File discovery:');
  console.error(`  project matched: ${discovery.projectMatched}`);
  console.error(`  selected project files: ${discovery.selectedProjectFiles}`);
  console.error(`  ignored by .gitignore: ${discovery.ignoredByGitignore}`);
  console.error(`  ignored by config: ${discovery.ignoredByConfig}`);
  console.error(`  parsed: ${discovery.parsed}`);
  console.error(`  skipped by extension/type: ${discovery.skippedByExtensionOrType}`);
  console.error(`  entries: ${discovery.entries}`);
  console.error(`  test leaf files: ${discovery.testLeafFiles}`);

  console.error('Entry matches:');
  for (const [pattern, matches] of Object.entries(diagnostics.entry.entryMatchesByPattern)) {
    console.error(`  ${pattern}: ${matches.length}`);
  }
  if (diagnostics.entry.zeroMatchPatterns.length > 0) {
    console.error(`  zero-match entry patterns: ${diagnostics.entry.zeroMatchPatterns.join(', ')}`);
  }

  const deadFiles = Object.entries(diagnostics.deadFiles);
  if (deadFiles.length > 0) {
    console.error('Dead file reasons:');
    for (const [path, reason] of deadFiles) {
      console.error(`  ${path}: ${reason.reason}`);
      console.error(
        `    entry=${reason.matchedEntry} test=${reason.matchedTestFilePatterns} imported=${reason.imported} onlyDeadOrTestImporters=${reason.onlyImportedByDeadOrTestFiles} testLeafSkipped=${reason.skippedFromReachabilityDueToTestLeafSemantics}`,
      );
      if (reason.importedBy.length > 0) {
        console.error(`    imported by: ${reason.importedBy.join(', ')}`);
      }
    }
  }

  if (diagnostics.fixPlan) {
    console.error('Fix plan:');
    console.error(`  delete files: ${diagnostics.fixPlan.filesToDelete.join(', ')}`);
    for (const [path, symbols] of Object.entries(diagnostics.fixPlan.filesWithExportEdits)) {
      console.error(`  edit exports in ${path}: ${symbols.join(', ')}`);
    }
    for (const [path, symbols] of Object.entries(diagnostics.fixPlan.skippedExportsInDeletedFiles)) {
      console.error(`  skip exports in deleted file ${path}: ${symbols.join(', ')}`);
    }
  }
}

function formatMode(mode: string): string {
  return mode ? `${mode[0].toUpperCase()}${mode.slice(1)}` : mode;
}

function hasIssues(analysis: AnalysisResult): boolean {
  return (
    Object.keys(analysis.issues.files).length > 0 ||
    Object.keys(analysis.issues.exports).length > 0 ||
    Object.keys(analysis.issues.unresolved ?? {}).length > 0
  );
}

function printTextReport(analysis: AnalysisResult): void {
  if (!hasIssues(analysis)) {
    console.log('No dead TypeScript code found');
    return;
  }

  const filePaths = Object.keys(analysis.issues.files);
  if (filePaths.length > 0) {
    console.log(`Unused files (${filePaths.length})`);
    for (const filePath of filePaths) {
      console.log(`  ${filePath}`);
    }
  }

  const exportCount = Object.values(analysis.issues.exports).reduce(
    (count, exports) => count + Object.keys(exports).length,
    0,
  );
  if (exportCount > 0) {
    console.log(`Unused exports (${exportCount})`);
    for (const [filePath, exports] of Object.entries(analysis.issues.exports)) {
      for (const issue of Object.values(exports)) {
        console.log(`  ${filePath}:${issue.line}:${issue.col} ${issue.symbol}`);
      }
    }
  }

  const unresolved = analysis.issues.unresolved ?? {};
  if (Object.keys(unresolved).length > 0) {
    console.log(`Unresolved imports (${analysis.counters.unresolved})`);
    for (const [filePath, imports] of Object.entries(unresolved)) {
      for (const importPath of imports) {
        console.log(`  ${filePath}: ${importPath}`);
      }
    }
  }
}

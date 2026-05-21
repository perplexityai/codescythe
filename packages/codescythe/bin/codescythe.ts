#!/usr/bin/env -S node --experimental-transform-types
'use strict';

type CliOptions = {
  config?: string;
  cwd?: string;
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

for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--json') {
    json = true;
  } else if (arg === '--fix') {
    shouldFix = true;
  } else if (arg === '--config' || arg === '-c') {
    options.config = args[++i];
  } else if (arg === '--directory' || arg === '-C') {
    options.cwd = args[++i];
  } else {
    console.error(`Unknown argument: ${arg}`);
    process.exit(2);
  }
}

const result = shouldFix ? fix(options) : analyze(options);
if (json) {
  console.log(JSON.stringify(result));
} else if (shouldFix) {
  const fixResult = result as FixResult;
  if (hasIssues(fixResult.analysis)) {
    printTextReport(fixResult.analysis);
    console.log('');
  }
  console.log(
    `Removed ${fixResult.removedExports} unused exports from ${fixResult.changedFiles.length} files and ${fixResult.removedFiles.length} unused files`,
  );
} else {
  printTextReport(result as AnalysisResult);
}

if (shouldFix) {
  const fixResult = result as FixResult;
  process.exit(hasIssues(fixResult.analysis) ? 1 : 0);
}

const analysisResult = result as AnalysisResult;
process.exit(hasIssues(analysisResult) ? 1 : 0);

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

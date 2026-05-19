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
  };
};

type FixResult = {
  changedFiles: string[];
  removedExports: number;
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
  console.log(`Removed ${fixResult.removedExports} unused exports from ${fixResult.changedFiles.length} files`);
} else if (result.counters.files === 0 && result.counters.exports === 0 && result.counters.unresolved === 0) {
  console.log('No dead TypeScript code found');
} else {
  const analysisResult = result as AnalysisResult;
  for (const filePath of Object.keys(analysisResult.issues.files)) {
    console.log(`unused file ${filePath}`);
  }
  for (const [filePath, exports] of Object.entries(analysisResult.issues.exports)) {
    for (const issue of Object.values(exports)) {
      console.log(`unused export ${filePath}:${issue.line}:${issue.col} ${issue.symbol}`);
    }
  }
}

if (shouldFix) {
  process.exit(0);
}

const analysisResult = result as AnalysisResult;
process.exit(analysisResult.counters.files || analysisResult.counters.exports || analysisResult.counters.unresolved ? 1 : 0);

'use strict';

type AnalyzeOptions = {
  config?: string;
  cwd?: string;
};

type NativeBinding = {
  analyze(options: AnalyzeOptions): string;
  fix(options: AnalyzeOptions): string;
};

const native = requireNative();

function analyze(options: AnalyzeOptions = {}) {
  return JSON.parse(native.analyze(options));
}

function fix(options: AnalyzeOptions = {}) {
  return JSON.parse(native.fix(options));
}

function requireNative(): NativeBinding {
  const packageName = nativePackageName();
  try {
    return require(packageName) as NativeBinding;
  } catch (error: unknown) {
    const hint = `Codescythe native package ${packageName} is not installed for ${process.platform}/${process.arch}.`;
    if (error instanceof Error) {
      error.message = `${hint}\n${error.message}`;
    }
    throw error;
  }
}

function nativePackageName(): string {
  if (process.platform === 'linux' && process.arch === 'x64') {
    return '@perplexity/codescythe-linux-amd64';
  }
  if (process.platform === 'linux' && process.arch === 'arm64') {
    return '@perplexity/codescythe-linux-arm64';
  }
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    return '@perplexity/codescythe-darwin-arm64';
  }
  throw new Error(`Codescythe does not ship a native package for ${process.platform}/${process.arch}`);
}

module.exports = {
  analyze,
  fix,
  native,
};

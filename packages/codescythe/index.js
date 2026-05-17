'use strict';

const native = requireNative();

function analyze(options) {
  return JSON.parse(native.analyze(options || {}));
}

function fix(options) {
  return JSON.parse(native.fix(options || {}));
}

function requireNative() {
  const packageName = nativePackageName();
  try {
    return require(packageName);
  } catch (error) {
    const hint = `Codescythe native package ${packageName} is not installed for ${process.platform}/${process.arch}.`;
    error.message = `${hint}\n${error.message}`;
    throw error;
  }
}

function nativePackageName() {
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

#!/usr/bin/env -S node --experimental-transform-types

const { existsSync, readFileSync } = require('node:fs');

const allowedTypes = [
  'build',
  'chore',
  'ci',
  'docs',
  'feat',
  'fix',
  'perf',
  'refactor',
  'revert',
  'style',
  'test',
] as const;

const allowedTypeSet = new Set<string>(allowedTypes);
const titlePattern =
  /^(?<type>[a-z]+)(?:\((?<scope>[-\w./@]+)\))?(?<breaking>!)?: (?<description>\S.*)$/;

function validateTitle(title: string): string | undefined {
  const trimmedTitle = title.trim();

  if (trimmedTitle.length === 0) {
    return 'PR title must not be empty.';
  }

  const match = titlePattern.exec(trimmedTitle);
  if (!match?.groups) {
    return 'PR title must match "type(scope)!: description".';
  }

  const type = match.groups.type;
  if (!allowedTypeSet.has(type)) {
    return `PR title type "${type}" is not allowed.`;
  }

  return undefined;
}

function getTitleFromArgs(args: string[]): string | undefined {
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];

    if (arg === '--title') {
      return args[index + 1] ?? '';
    }

    if (arg.startsWith('--title=')) {
      return arg.slice('--title='.length);
    }
  }

  return undefined;
}

function getTitleFromGitHubEvent(): string | undefined {
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath || !existsSync(eventPath)) {
    return undefined;
  }

  try {
    const event = JSON.parse(readFileSync(eventPath, 'utf8'));
    const title = event?.pull_request?.title;
    return typeof title === 'string' ? title : undefined;
  } catch (error) {
    if (process.env.GITHUB_EVENT_NAME === 'pull_request') {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Could not read GitHub event payload: ${message}`);
      process.exit(1);
    }

    return undefined;
  }
}

function getTitleFromEnv(name: string): string | undefined {
  const title = process.env[name];
  if (title === undefined) {
    return undefined;
  }

  if (title.length > 0) {
    return title;
  }

  return process.env.GITHUB_EVENT_NAME === 'pull_request' ? '' : undefined;
}

function getPrTitle(): string | undefined {
  const argTitle = getTitleFromArgs(process.argv.slice(2));
  if (argTitle !== undefined) {
    return argTitle;
  }

  const prTitle = getTitleFromEnv('PR_TITLE');
  if (prTitle !== undefined) {
    return prTitle;
  }

  const githubPrTitle = getTitleFromEnv('GITHUB_PR_TITLE');
  if (githubPrTitle !== undefined) {
    return githubPrTitle;
  }

  return getTitleFromGitHubEvent();
}

const title = getPrTitle();
if (title === undefined) {
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') {
    console.error('Could not determine the pull request title.');
    process.exit(1);
  }

  process.exit(0);
}

const error = validateTitle(title);
if (error) {
  console.error(`Invalid PR title: "${title}"`);
  console.error(error);
  console.error(`Allowed types: ${allowedTypes.join(', ')}`);
  console.error(
    'Examples: "feat: add query output", "fix(cli): handle missing config"',
  );
  process.exit(1);
}

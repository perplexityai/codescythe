"""Wait for successful main CI at the exact release commit."""
import argparse
import json
import os
import re
import subprocess
import time
import urllib.request


def check_runs(runs, repo, commit):
    matching = [run for run in runs if run['event'] == 'push'
                and run['head_branch'] == 'main' and run['head_sha'] == commit
                and run['head_repository']['full_name'] == repo]
    if not matching:
        return False
    run = max(matching, key=lambda item: (item['run_number'], item['run_attempt']))
    if run['status'] != 'completed':
        return False
    if run['conclusion'] != 'success':
        raise ValueError('Main CI failed: ' + run['html_url'])
    print('Main CI passed: ' + run['html_url'])
    return True


def workflow_runs(repo, commit):
    request = urllib.request.Request(
        f'https://api.github.com/repos/{repo}/actions/workflows/ci.yml/runs?head_sha={commit}&event=push&per_page=100',
        headers={'Authorization': f"Bearer {os.environ['GH_TOKEN']}",
                 'Accept': 'application/vnd.github+json', 'User-Agent': 'gha-release-gate'})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)['workflow_runs']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--expected-commit')
    parser.add_argument('--disk_cache')
    parser.add_argument('--repository_cache')
    args = parser.parse_args()
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if args.expected_commit:
        if not re.fullmatch(r'[0-9a-f]{40}', args.expected_commit) or commit != args.expected_commit:
            raise ValueError('Release tag moved after CI verification')
        return
    repo = os.environ['GITHUB_REPOSITORY']
    if repo != 'perplexityai/codescythe':
        raise ValueError('Unexpected release repository')
    tag = os.environ.get('RELEASE_TAG')
    if tag:
        if not re.fullmatch(r'codescythe_cli_v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?', tag):
            raise ValueError('Unexpected release tag')
        tagged = subprocess.check_output(['git', 'rev-parse', f'refs/tags/{tag}^{{commit}}'], text=True).strip()
        if tagged != commit:
            raise ValueError('Checkout does not match release tag')
    subprocess.run(['git', 'merge-base', '--is-ancestor', commit, 'origin/main'], check=True)
    deadline = time.monotonic() + 90 * 60
    while time.monotonic() < deadline:
        if check_runs(workflow_runs(repo, commit), repo, commit):
            if os.environ.get('GITHUB_OUTPUT'):
                with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
                    output.write(f'commit={commit}\n')
            return
        print(f'Waiting for main CI at {commit}', flush=True)
        time.sleep(20)
    raise TimeoutError('Main CI missing or pending after 90 minutes')


if __name__ == '__main__':
    main()

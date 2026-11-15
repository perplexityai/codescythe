"""Reject unrelated, stale, and failed release validation."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('gate', Path(__file__).with_name('gha-release-gate.py'))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

class GateTests(unittest.TestCase):
    def run_fixture(self, **changes):
        return dict(event='push', head_branch='main', head_sha='a' * 40,
                    head_repository={'full_name': 'org/repo'}, run_number=1,
                    run_attempt=1, status='completed', conclusion='success',
                    html_url='https://github.com/org/repo/actions/runs/1', **changes)

    def check(self, runs):
        return gate.check_runs(runs, 'org/repo', 'a' * 40)

    def test_success(self):
        self.assertTrue(self.check([self.run_fixture()]))

    def test_missing(self):
        self.assertFalse(self.check([]))

    def test_unrelated(self):
        for changes in [dict(event='pull_request'), dict(head_branch='feature'),
                        dict(head_sha='b' * 40), dict(head_repository={'full_name': 'fork/repo'})]:
            run = self.run_fixture(); run.update(changes)
            self.assertFalse(self.check([run]))

    def test_pending(self):
        run = self.run_fixture(); run.update(status='in_progress', conclusion=None)
        self.assertFalse(self.check([run]))

    def test_failed_or_cancelled(self):
        for conclusion in ['failure', 'cancelled', 'skipped', 'timed_out']:
            run = self.run_fixture(); run['conclusion'] = conclusion
            with self.assertRaises(ValueError): self.check([run])

    def test_latest_run_wins(self):
        latest = self.run_fixture(); latest.update(run_number=2, conclusion='failure')
        with self.assertRaises(ValueError): self.check([self.run_fixture(), latest])

    def test_latest_attempt_wins(self):
        latest = self.run_fixture(); latest.update(run_attempt=2, status='in_progress', conclusion=None)
        self.assertFalse(self.check([latest, self.run_fixture()]))

if __name__ == '__main__':
    unittest.main()

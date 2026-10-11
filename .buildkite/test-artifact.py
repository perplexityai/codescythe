import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('artifact', Path(__file__).with_name('verify-artifact.py'))
artifact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifact)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        os.environ['BUILDKITE_BUILD_ID'] = 'test-build'
        os.environ['BUILDKITE_PIPELINE_SLUG'] = 'codescythe'
        names = ['codescythe-linux-amd64', 'codescythe-linux-arm64',
                 'codescythe-darwin-arm64', 'checksums.txt']
        for name in names:
            (self.root / name).write_bytes(b'verified binary')
        self.manifest = {
            'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            'build_id': 'test-build', 'pipeline': 'codescythe',
            'files': {name: hashlib.sha256((self.root / name).read_bytes()).hexdigest()
                      for name in names},
        }

    def verify(self):
        (self.root / 'buildkite-release.json').write_text(json.dumps(self.manifest))
        artifact.verify(self.root)

    def test_valid(self):
        self.verify()

    def test_wrong_identity(self):
        for key in ['commit', 'build_id', 'pipeline']:
            with self.subTest(key=key):
                original = self.manifest[key]
                self.manifest[key] = 'wrong'
                with self.assertRaisesRegex(ValueError, 'mismatch'):
                    self.verify()
                self.manifest[key] = original

    def test_tampered_binary(self):
        (self.root / 'codescythe-linux-amd64').write_bytes(b'poison')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            self.verify()

    def test_missing_binary(self):
        (self.root / 'codescythe-linux-amd64').unlink()
        with self.assertRaisesRegex(ValueError, 'Unexpected artifact set'):
            self.verify()

    def test_symlink(self):
        path = self.root / 'codescythe-linux-amd64'
        path.unlink()
        path.symlink_to(self.root / 'codescythe-linux-arm64')
        with self.assertRaisesRegex(ValueError, 'Missing or unsafe'):
            self.verify()

    def test_path_traversal(self):
        self.manifest['files']['../binary'] = 'invalid'
        with self.assertRaisesRegex(ValueError, 'Unexpected artifact set'):
            self.verify()

    def test_extra_artifact(self):
        (self.root / 'extra').write_bytes(b'not covered by manifest')
        with self.assertRaisesRegex(ValueError, 'Unexpected artifact set'):
            self.verify()

    def test_incomplete_manifest(self):
        del self.manifest['files']['codescythe-linux-amd64']
        with self.assertRaisesRegex(ValueError, 'Unexpected artifact set'):
            self.verify()


if __name__ == '__main__':
    unittest.main()

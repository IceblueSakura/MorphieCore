"""Acquisition boundary checks; all inputs are local and no network is permitted."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import assets


class AssetContracts(unittest.TestCase):
    def test_checked_in_bytes_and_literal_derivations_are_offline(self):
        with patch.object(assets, "download", side_effect=AssertionError("network")):
            self.assertGreater(assets.verify(), 0)

    def test_missing_changed_and_retargeted_assets_do_not_silently_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copytree(assets.ROOT / "tests/upstream", root / "tests/upstream")
            shutil.copytree(assets.ROOT / "testdata/upstream", root / "testdata/upstream")
            manifest = json.loads((root / "tests/upstream/assets.lock.json").read_text())
            path = root / "testdata/upstream" / manifest["files"][0]["local"]
            original = path.read_bytes()
            with patch.object(assets, "download", side_effect=AssertionError("network")):
                path.unlink()
                with self.assertRaisesRegex(ValueError, "missing/changed asset"):
                    assets.verify(root)
                path.write_bytes(original + b"changed")
                with self.assertRaisesRegex(ValueError, "missing/changed asset"):
                    assets.verify(root)
                path.write_bytes(original)
                lock_path = root / "tests/upstream/sources.lock.json"
                lock = json.loads(lock_path.read_text())
                lock["sources"][manifest["files"][0]["source"]]["revision"] = "0" * 40
                lock_path.write_text(json.dumps(lock))
                with self.assertRaisesRegex(ValueError, "pin mismatch"):
                    assets.verify(root)

    def test_paths_cannot_escape_or_follow_symlinks(self):
        for value in ["", "../outside", "/absolute", "a/../b", "a//b"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                assets.relative(value)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "link").symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "symlink"):
                assets.local_path(root, "link/file")

    def test_extraction_is_literal_only_and_rejects_changed_expressions(self):
        source = (assets.ROOT / "testdata/upstream/eventsource-parser/fixtures.ts").read_text()
        self.assertEqual(assets.literal_chunks(source, "getCarriageReturnFixtureStream"),
                         b"data: dog\rdata: bark\r\rdata: cat\rdata: meow\r\r")
        changed = source.replace("onChunk('data: dog\\r')", "onChunk(execute())")
        with self.assertRaisesRegex(ValueError, "unreviewed framing expression"):
            assets.literal_chunks(changed, "getCarriageReturnFixtureStream")
        with self.assertRaisesRegex(ValueError, "unreviewed framing function"):
            assets.literal_chunks(source, "getTimeFixtureStream")

    def test_restore_checks_download_digest_before_writing(self):
        entry = {"revision": "a" * 40, "path": "fixture", "sha256": "0" * 64}
        response = unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value = b"changed"
        opener = unittest.mock.Mock()
        opener.open.return_value = response
        with patch.object(assets.urllib.request, "build_opener", return_value=opener):
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                assets.download({"repository": "https://github.com/example/fixtures"}, entry)
        with self.assertRaisesRegex(ValueError, "untrusted"):
            assets.download({"repository": "https://example.test/private"}, entry)


if __name__ == "__main__":
    unittest.main()

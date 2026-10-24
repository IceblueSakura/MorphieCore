"""Latest candidates are visible metadata, never implicit pin/asset updates."""
import copy
import unittest
from unittest.mock import Mock

import candidates


class CandidateContracts(unittest.TestCase):
    def setUp(self):
        self.source = {"selection": "stable-release", "repository": "https://github.com/vercel/ai",
                       "release": "@ai-sdk/openai@4.0.101", "revision": "a" * 40,
                       "package": "@ai-sdk/openai"}

    def test_registry_monorepo_tag_and_annotated_commit_are_checked_without_mutation(self):
        original = copy.deepcopy(self.source)
        fetch = Mock(side_effect=[{"version": "4.0.101"},
                                  {"object": {"type": "tag", "sha": "b" * 40}},
                                  {"object": {"type": "commit", "sha": "a" * 40}}])
        self.assertEqual(candidates.candidate("vercel-openai", self.source, fetch)["status"], "unchanged")
        self.assertIn("%40ai-sdk%2Fopenai%404.0.101", fetch.call_args_list[1].args[0])
        self.assertEqual(self.source, original)

    def test_release_or_commit_drift_requires_review(self):
        for version, sha in [("4.0.102", "a" * 40), ("4.0.101", "c" * 40)]:
            fetch = Mock(side_effect=[{"version": version}, {"object": {"type": "commit", "sha": sha}}])
            self.assertEqual(candidates.candidate("vercel-openai", self.source, fetch)["status"],
                             "review-required")

    def test_snapshots_do_not_claim_latest_and_lookup_failures_do_not_fallback(self):
        fetch = Mock(side_effect=ValueError("missing tag"))
        with self.assertRaisesRegex(ValueError, "missing tag"):
            candidates.candidate("vercel-openai", self.source, fetch)
        self.source["selection"] = "source-snapshot-no-release"
        fetch.reset_mock()
        self.assertEqual(candidates.candidate("wpt", self.source, fetch)["status"],
                         "snapshot-not-latest-release")
        fetch.assert_not_called()
        self.source["selection"] = "unrecognized"
        with self.assertRaisesRegex(ValueError, "unknown source selection"):
            candidates.candidate("wpt", self.source, fetch)

    def test_non_stable_or_unresolved_candidates_fail(self):
        for replies in [[{"version": "4.1.0-beta.1"}],
                        [{"version": "4.0.101"}, {"object": {"type": "tree", "sha": "a" * 40}}]]:
            with self.assertRaises(ValueError):
                candidates.candidate("vercel-openai", self.source, Mock(side_effect=replies))
        self.source.pop("package")
        with self.assertRaisesRegex(ValueError, "non-stable release"):
            candidates.candidate("pi", self.source, Mock(return_value={"prerelease": True, "draft": False}))


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
import subprocess
import tempfile
import unittest
from pathlib import Path

from verify_index import (
    check_ratio,
    is_excluded,
    pattern_matches,
    tracked_source_paths,
)


class ExclusionTests(unittest.TestCase):
    def test_glob_matches_root_and_nested_minified_files(self):
        self.assertTrue(pattern_matches("bundle.min.js", "**/*.min.js"))
        self.assertTrue(pattern_matches("assets/bundle.min.js", "**/*.min.js"))
        self.assertFalse(pattern_matches("assets/bundle.js", "**/*.min.js"))

    def test_directory_and_negation_follow_order(self):
        patterns = ["vendor/", "!vendor/keep.js"]
        self.assertTrue(is_excluded("vendor/drop.js", patterns))
        self.assertFalse(is_excluded("vendor/keep.js", patterns))

    def test_tracked_paths_honor_gitignore_config_and_typescript_variants(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            files = {
                ".gitignore": ".claude\n",
                ".claude/release.js": "ignored()\n",
                "src/main.ts": "export const ts = true\n",
                "src/config.mts": "export const mts = true\n",
                "vendor/bundle.min.js": "minified()\n",
            }
            for relative, content in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "-f", *files], cwd=root, check=True)

            expected = tracked_source_paths(str(root), ["**/*.min.js"])

            self.assertEqual(expected["typescript"], {"src/main.ts", "src/config.mts"})
            self.assertEqual(expected["javascript"], set())


class CeilingTests(unittest.TestCase):
    def test_ratio_at_ceiling_passes_and_regression_fails(self):
        note = lambda *_: None
        self.assertTrue(check_ratio(note, "risk", 112, 1000, 0.100, 0.112))
        self.assertFalse(check_ratio(note, "risk", 113, 1000, 0.100, 0.112))
        self.assertFalse(check_ratio(note, "risk", 0, 0, 0.100, 0.112))


if __name__ == "__main__":
    unittest.main()

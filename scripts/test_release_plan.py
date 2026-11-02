import unittest

from release_plan import release_plan


class ReleasePlanTests(unittest.TestCase):
    def test_main_push_publishes_unique_test_signed_beta(self):
        plan = release_plan("0.10.0", "push", "refs/heads/main", "42")
        self.assertEqual(plan["tag"], "v0.10.0-beta.42")
        self.assertEqual(plan["publish"], "true")
        self.assertEqual(plan["prerelease"], "true")
        self.assertEqual(plan["production_signing"], "false")
        self.assertNotEqual(
            plan["tag"], release_plan("0.10.0", "push", "refs/heads/main", "43")["tag"]
        )

    def test_stable_tag_requires_production_signing(self):
        plan = release_plan("0.10.0", "push", "refs/tags/v0.10.0", "44")
        self.assertEqual(plan["tag"], "v0.10.0")
        self.assertEqual(plan["publish"], "true")
        self.assertEqual(plan["prerelease"], "false")
        self.assertEqual(plan["production_signing"], "true")

    def test_explicit_beta_tag_keeps_test_signing(self):
        plan = release_plan("0.10.0", "push", "refs/tags/v0.10.0-beta.42", "45")
        self.assertEqual(plan["tag"], "v0.10.0-beta.42")
        self.assertEqual(plan["prerelease"], "true")
        self.assertEqual(plan["production_signing"], "false")

    def test_manual_runs_never_publish_or_require_production_secrets(self):
        for ref in ["refs/heads/main", "refs/tags/v0.10.0", "refs/tags/v0.10.0-beta.42"]:
            with self.subTest(ref=ref):
                plan = release_plan("0.10.0", "workflow_dispatch", ref, "46")
                self.assertEqual(plan["publish"], "false")
                self.assertEqual(plan["production_signing"], "false")
                self.assertEqual(plan["tag"], "")

    def test_mismatched_or_unexpected_tags_fail_before_building(self):
        for tag in ["v0.9.0", "v0.9.0-beta.42", "v0.10.0-beta.0", "v0.10.0-rc.1"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release_plan("0.10.0", "push", f"refs/tags/{tag}", "47")

    def test_other_branches_cannot_publish(self):
        with self.assertRaises(ValueError):
            release_plan("0.10.0", "push", "refs/heads/feature/example", "48")

    def test_output_values_cannot_inject_actions_outputs(self):
        for version, run in [("0.10.0\npublish=true", "49"), ("0.10.0", "49\npublish=true")]:
            with self.subTest(version=version, run=run), self.assertRaises(ValueError):
                release_plan(version, "push", "refs/heads/main", run)


if __name__ == "__main__":
    unittest.main()

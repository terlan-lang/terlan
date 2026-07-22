import unittest

from makefile_contract import make_target_prerequisites


class MakeTargetPrerequisitesTest(unittest.TestCase):
    def test_reads_single_line_prerequisites(self) -> None:
        makefile = "check: rust-test-suite check-gates\n"

        self.assertEqual(
            ["rust-test-suite", "check-gates"],
            make_target_prerequisites(makefile, "check"),
        )

    def test_reads_continued_prerequisites_without_recipe_lines(self) -> None:
        makefile = (
            "http-check: \\\n"
            "\tvm-stream-check \\\n"
            "\tvm-router-check\n"
            "\tcargo test http\n"
        )

        self.assertEqual(
            ["vm-stream-check", "vm-router-check"],
            make_target_prerequisites(makefile, "http-check"),
        )

    def test_reports_missing_target(self) -> None:
        self.assertIsNone(make_target_prerequisites("check:\n", "missing"))


if __name__ == "__main__":
    unittest.main()

"""`diff-all`'s verdict: each report on its own, then the formant corpus."""

import unittest

from oracle.cli import formant_aggregate, verdict
from oracle.diff import DiffReport


def formant_report(checked: int, violations: int) -> DiffReport:
    from oracle import tolerances as tol

    return DiffReport(
        measure="formant",
        passed=violations / checked <= tol.FORMANT_CORPUS_VIOLATION_RATE_MAX,
        summary={"checked_points": checked, "violations": violations, "missing_points": 0},
        violations=[],
        notes=[],
    )


def other_report(passed: bool) -> DiffReport:
    return DiffReport(measure="pitch", passed=passed, summary={}, violations=[], notes=[])


class VerdictTest(unittest.TestCase):
    def test_one_failing_fixture_fails_the_run_even_when_the_corpus_passes(self):
        reports = [formant_report(1000, 20), formant_report(9000, 0)]
        self.assertTrue(formant_aggregate(reports)[0])  # 0.2% over the corpus
        self.assertFalse(reports[0].passed)  # 2% on its own
        self.assertFalse(verdict(reports))

    def test_passing_fixtures_and_corpus_pass(self):
        reports = [formant_report(1530, 8), formant_report(5000, 0), other_report(True)]
        self.assertTrue(verdict(reports))

    def test_any_other_measure_failing_fails_the_run(self):
        self.assertFalse(verdict([formant_report(1000, 0), other_report(False)]))

    def test_a_missing_or_malformed_file_fails_the_run(self):
        self.assertFalse(verdict([other_report(True)], structural_failure=True))

    def test_no_formant_reports_means_no_aggregate(self):
        self.assertIsNone(formant_aggregate([other_report(True)]))
        self.assertTrue(verdict([other_report(True)]))


if __name__ == "__main__":
    unittest.main()

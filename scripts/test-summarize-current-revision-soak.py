#!/usr/bin/env python3

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("summarize-current-revision-soak.py")
SPEC = importlib.util.spec_from_file_location("soak_summary", SCRIPT)
SOAK_SUMMARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOAK_SUMMARY)


def samples(pid, values):
    return [{"pid": pid, "fds": value} for value in values]


class SoakSummaryTest(unittest.TestCase):
    def test_latest_process_generation_excludes_planned_restart(self):
        values = samples(101, [300, 320, 340]) + samples(202, [180, 220, 240])

        self.assertEqual(
            SOAK_SUMMARY.latest_process_generation(values), samples(202, [180, 220, 240])
        )

    def test_terminal_stability_accepts_connection_pool_warmup_plateau(self):
        values = samples(
            202,
            [188, 216, 236, 377, 432, 403, 418, 431, 446, 456, 465, 465]
            + [465, 466, 469, 538, 545, 551, 551, 561, 582, 634]
            + [592, 592, 592, 595, 600, 598, 599, 599, 599],
        )

        result = SOAK_SUMMARY.assert_terminal_stability("postgres", values, "fds", 32)

        self.assertEqual(result["mode"], "terminal-stability")
        self.assertLessEqual(result["end_window_median"], result["limit"])

    def test_terminal_stability_rejects_continuing_growth(self):
        values = samples(202, list(range(100, 500, 10)))

        with self.assertRaisesRegex(AssertionError, "exceeded stability limit"):
            SOAK_SUMMARY.assert_terminal_stability("postgres", values, "fds", 32)


if __name__ == "__main__":
    unittest.main()

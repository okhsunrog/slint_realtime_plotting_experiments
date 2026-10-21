import importlib.util
import sys
from pathlib import Path
import unittest

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("motion", Path(__file__).with_name("analyze-motion.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MotionTests(unittest.TestCase):
    def report(self, motion):
        return {"query": "?motion=clocked&rate=1000", "motion": motion}

    def test_uniform_raf_can_hide_data_starvation(self):
        result = module.analyze(self.report([[0, 0, 16, 16], [16, 16, 16, 0],
                                            [32, 16, 16, 0], [48, 32, 48, 16]]))
        self.assertEqual(result["flat_head_intervals"], 1)
        self.assertEqual(result["starved_flat_intervals"], 1)
        self.assertEqual(result["flat_with_raf_under25"], 1)

    def test_late_raf_does_not_mean_the_display_head_stopped(self):
        result = module.analyze(self.report([[0, 0, 100, 100], [50, 50, 150, 100]]))
        self.assertEqual(result["flat_head_intervals"], 0)
        self.assertEqual(result["speed_ratio_min"], 1)

    def test_distinguish_frozen_history_from_missing_received_samples(self):
        result = module.analyze(self.report([[0, 0, 100, 100], [16, 0, 116, 116]]))
        self.assertEqual(result["flat_head_intervals"], 1)
        self.assertEqual(result["starved_flat_intervals"], 0)

    def test_receipts_crossing_sample_boundary_are_not_fabricated(self):
        report = self.report([])
        report["flow"] = [[1, "receipt", 0, 64, 0], [2, "consumed", 1, 64, 1],
                          [32, "receipt", 1, 64, 0], [33, "consumed", 2, 64, 1]]
        result = module.analyze(report)
        self.assertEqual(result["matched_worker_receipts"], 1)
        self.assertEqual(result["worker_receipt_max_ms"], 30)

    def test_reject_invented_data(self):
        with self.assertRaises(ValueError):
            module.analyze(self.report([[0, 0, 0, 0], [16, 16, 8, 0]]))


if __name__ == "__main__":
    unittest.main()

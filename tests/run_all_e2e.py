#!/usr/bin/env python3
"""
Unified E2E Test Runner for Binance Futures Bot
Derives strictly from user requirements in ORIGINAL_REQUEST.md and PROJECT.md Feature Inventory.
Runs all 4 Tiers (84 tests) and returns exit code 0 on full pass.
"""

import os
import sys
import time
import unittest

# Ensure project root is in sys.path
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if PROJECT_ROOT not in sys.path:
    sys.path.insert(0, PROJECT_ROOT)

from tests.test_tier1_features import (
    TestTier1Feature1DataIngestionCaching,
    TestTier1Feature2DurationTracking,
    TestTier1Feature3TimeDecayExits,
    TestTier1Feature4TimeWeightedOptimization,
    TestTier1Feature5ComparativeRanking,
    TestTier1Feature6RunnableDeploymentArtifacts,
    TestTier1Feature7FullE2EVerification,
)
from tests.test_tier2_boundaries import (
    TestTier2Feature1DataIngestionBoundaries,
    TestTier2Feature2DurationTrackingBoundaries,
    TestTier2Feature3TimeDecayBoundaries,
    TestTier2Feature4StrategyOptimizationBoundaries,
    TestTier2Feature5ComparativeRankingBoundaries,
    TestTier2Feature6RunnableDeploymentBoundaries,
    TestTier2Feature7FullE2EVerificationBoundaries,
)
from tests.test_tier3_cross_features import TestTier3CrossFeatureCombinations
from tests.test_tier4_real_world import TestTier4RealWorldScenarios


def build_suite_for_tier(tier: int) -> unittest.TestSuite:
    suite = unittest.TestSuite()
    loader = unittest.TestLoader()

    if tier == 1:
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature1DataIngestionCaching))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature2DurationTracking))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature3TimeDecayExits))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature4TimeWeightedOptimization))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature5ComparativeRanking))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature6RunnableDeploymentArtifacts))
        suite.addTests(loader.loadTestsFromTestCase(TestTier1Feature7FullE2EVerification))
    elif tier == 2:
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature1DataIngestionBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature2DurationTrackingBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature3TimeDecayBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature4StrategyOptimizationBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature5ComparativeRankingBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature6RunnableDeploymentBoundaries))
        suite.addTests(loader.loadTestsFromTestCase(TestTier2Feature7FullE2EVerificationBoundaries))
    elif tier == 3:
        suite.addTests(loader.loadTestsFromTestCase(TestTier3CrossFeatureCombinations))
    elif tier == 4:
        suite.addTests(loader.loadTestsFromTestCase(TestTier4RealWorldScenarios))
    return suite


def run_all_tiers():
    print("=" * 80)
    print("🚀 BINANCE FUTURES BOT — 4-TIER E2E TEST RUNNER")
    print("Derivation: ORIGINAL_REQUEST.md & PROJECT.md Feature Inventory")
    print("=" * 80)

    tiers = [
        (1, "Tier 1: Feature Coverage (7 Features x 5 Tests)", build_suite_for_tier(1)),
        (2, "Tier 2: Boundary & Corner Cases (7 Features x 5 Tests)", build_suite_for_tier(2)),
        (3, "Tier 3: Cross-Feature Combinations (Pairwise & Multi-Feature)", build_suite_for_tier(3)),
        (4, "Tier 4: Real-World Application Scenarios (10-Day & Multi-Coin)", build_suite_for_tier(4)),
    ]

    total_run = 0
    total_passed = 0
    total_failed = 0
    total_errors = 0
    tier_results = []

    start_all = time.time()

    for tier_num, tier_name, suite in tiers:
        print(f"\n▶ Executing {tier_name} ({suite.countTestCases()} test cases)...")
        runner = unittest.TextTestRunner(verbosity=1)
        tier_start = time.time()
        result = runner.run(suite)
        tier_duration = time.time() - tier_start

        run_count = result.testsRun
        fail_count = len(result.failures)
        err_count = len(result.errors)
        pass_count = run_count - (fail_count + err_count)

        total_run += run_count
        total_passed += pass_count
        total_failed += fail_count
        total_errors += err_count

        tier_status = "PASS" if (fail_count == 0 and err_count == 0) else "FAIL"
        tier_results.append({
            "tier": tier_num,
            "name": tier_name,
            "count": run_count,
            "passed": pass_count,
            "failed": fail_count,
            "errors": err_count,
            "duration": tier_duration,
            "status": tier_status,
        })

    total_duration = time.time() - start_all

    print("\n" + "=" * 80)
    print("📊 4-TIER E2E TEST EXECUTION SUMMARY")
    print("=" * 80)
    print(f"{'Tier':<8}{'Description':<46}{'Tests':<8}{'Passed':<8}{'Failed':<8}{'Status':<8}")
    print("-" * 80)

    for tr in tier_results:
        print(f"Tier {tr['tier']:<3}{tr['name'][:44]:<46}{tr['count']:<8}{tr['passed']:<8}{tr['failed']:<8}{tr['status']:<8}")

    print("-" * 80)
    overall_status = "SUCCESS (100% PASS)" if (total_failed == 0 and total_errors == 0) else "FAILED"
    print(f"{'TOTAL':<8}{'All 4 Tiers Combined':<46}{total_run:<8}{total_passed:<8}{total_failed:<8}{overall_status:<8}")
    print(f"Total Execution Time: {total_duration:.3f} seconds")
    print("=" * 80)

    if total_failed == 0 and total_errors == 0:
        print("\n✅ ALL TESTS PASSED. Exit Code: 0\n")
        return 0
    else:
        print(f"\n❌ TESTS FAILED: {total_failed} failures, {total_errors} errors. Exit Code: 1\n")
        return 1


if __name__ == "__main__":
    exit_code = run_all_tiers()
    sys.exit(exit_code)

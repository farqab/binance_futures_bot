"""
Milestone 1 Feature 2: Time-in-Trade & Duration Tracking Instrumentation.
Exposes TradeRecord, DurationMetricsCalculator, and formatting helpers.
Path: strategies/duration_tracker.py
"""

from data.kline_cache import TradeRecord, DurationMetricsCalculator

__all__ = ["TradeRecord", "DurationMetricsCalculator"]

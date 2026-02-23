#!/usr/bin/env python3
"""
Monte Carlo profiling for kolbot Chrome -> agent messaging latency.

Simulates timing across the pipeline with realistic variance:
1. Chrome popup.js user action
2. background.js message routing
3. Native messaging stdin write/read
4. Agent command channel recv()
5. Config parsing + hot-swap
6. Next decision using new config

Author: Claude Code latency analyzer
"""

import random
import statistics
from dataclasses import dataclass
from typing import List
import math

@dataclass
class LatencyComponents:
    """Individual latency segments (ms)"""
    chrome_popup_to_background: float      # ~0.1ms (same extension)
    background_to_native_msg: float        # ~0.2-0.5ms (extension API overhead)
    native_msg_encode: float               # ~0.05ms (4-byte length + JSON encode)
    native_msg_transport: float            # ~0.5-2ms (stdio pipe, OS buffering)
    native_msg_decode: float               # ~0.1-0.5ms (parse 4-byte length + JSON decode)
    agent_loop_detection: float            # ~0-40ms (depends on tick phase when msg arrives)
    config_json_to_yaml: float             # ~0.2-0.8ms (depends on config size)
    config_parse: float                    # ~0.5-2ms (serde_yaml deserialization)
    config_reload: float                   # ~0.1-0.5ms (swapping pointers in GameManager)
    total: float = 0.0

    def calculate_total(self) -> float:
        """Sum all components"""
        self.total = sum([
            self.chrome_popup_to_background,
            self.background_to_native_msg,
            self.native_msg_encode,
            self.native_msg_transport,
            self.native_msg_decode,
            self.agent_loop_detection,
            self.config_json_to_yaml,
            self.config_parse,
            self.config_reload,
        ])
        return self.total


def normal_clamp(mean: float, stddev: float, min_val: float = 0, max_val: float = None) -> float:
    """Sample from normal distribution, clamped to [min, max]"""
    val = random.gauss(mean, stddev)
    val = max(val, min_val)
    if max_val is not None:
        val = min(val, max_val)
    return val


def sample_latency_profile(config_size_bytes: int = 5000) -> LatencyComponents:
    """
    Sample one latency measurement from realistic distributions.

    Args:
        config_size_bytes: Size of config being updated (affects parsing time)
    """
    # Chrome popup → background (same-process, very fast)
    chrome_popup_to_background = normal_clamp(0.1, 0.05, 0.02, 0.3)

    # background.js → native messaging (extension API call)
    background_to_native = normal_clamp(0.3, 0.15, 0.1, 1.0)

    # Encode: 4-byte length prefix + JSON
    native_encode = normal_clamp(0.05, 0.02, 0.02, 0.2)

    # Transport through stdio pipe
    # This is where OS buffering and IPC scheduling happens
    # Most messages go through immediately (~0.5-1ms)
    # But blocking conditions can add variance
    native_transport = normal_clamp(1.0, 1.5, 0.3, 5.0)

    # Decode: parse 4-byte length, then JSON
    native_decode = normal_clamp(0.2, 0.1, 0.05, 0.6)

    # **CRITICAL**: Agent loop detection
    # The agent checks cmd_rx.try_recv() every 40ms tick
    # If message arrives in the middle of a tick, worst case is 39ms wait
    # Average case is ~20ms (uniformly distributed in a tick)
    # Best case is ~0ms (message arrives right before check)
    loop_detection = random.uniform(0, 40)  # Uniformly distributed in one tick

    # JSON → YAML conversion (serde_json::to_string)
    # Typical config ~5KB
    config_json_to_yaml = normal_clamp(0.3, 0.1, 0.1, 1.0)

    # YAML → AgentConfig deserialization (serde_yaml::from_str)
    # Scales roughly with config size
    size_factor = min(config_size_bytes / 5000, 3.0)  # Cap at 3x for pathological configs
    config_parse = normal_clamp(1.0 * size_factor, 0.5 * size_factor, 0.2, 5.0)

    # reload_config: swapping Arc<AgentConfig> in GameManager
    # This is just a pointer swap, very fast
    config_reload = normal_clamp(0.2, 0.1, 0.05, 0.5)

    lat = LatencyComponents(
        chrome_popup_to_background=chrome_popup_to_background,
        background_to_native_msg=background_to_native,
        native_msg_encode=native_encode,
        native_msg_transport=native_transport,
        native_msg_decode=native_decode,
        agent_loop_detection=loop_detection,
        config_json_to_yaml=config_json_to_yaml,
        config_parse=config_parse,
        config_reload=config_reload,
    )
    lat.calculate_total()
    return lat


def monte_carlo_profile(num_runs: int = 10000, config_size: int = 5000) -> dict:
    """
    Run Monte Carlo simulation of latency distribution.

    Returns:
        Dictionary with statistics
    """
    results = []
    component_breakdown = {
        'chrome_popup_to_background': [],
        'background_to_native_msg': [],
        'native_msg_encode': [],
        'native_msg_transport': [],
        'native_msg_decode': [],
        'agent_loop_detection': [],
        'config_json_to_yaml': [],
        'config_parse': [],
        'config_reload': [],
    }

    for _ in range(num_runs):
        sample = sample_latency_profile(config_size)
        results.append(sample.total)

        component_breakdown['chrome_popup_to_background'].append(sample.chrome_popup_to_background)
        component_breakdown['background_to_native_msg'].append(sample.background_to_native_msg)
        component_breakdown['native_msg_encode'].append(sample.native_msg_encode)
        component_breakdown['native_msg_transport'].append(sample.native_msg_transport)
        component_breakdown['native_msg_decode'].append(sample.native_msg_decode)
        component_breakdown['agent_loop_detection'].append(sample.agent_loop_detection)
        component_breakdown['config_json_to_yaml'].append(sample.config_json_to_yaml)
        component_breakdown['config_parse'].append(sample.config_parse)
        component_breakdown['config_reload'].append(sample.config_reload)

    results.sort()

    # Calculate percentiles
    percentiles = [1, 5, 10, 25, 50, 75, 90, 95, 99]
    percentile_vals = {
        p: results[int(len(results) * p / 100)]
        for p in percentiles
    }

    # Component stats
    component_stats = {}
    for name, values in component_breakdown.items():
        component_stats[name] = {
            'mean': statistics.mean(values),
            'median': statistics.median(values),
            'stdev': statistics.stdev(values) if len(values) > 1 else 0,
            'min': min(values),
            'max': max(values),
            'p95': values[int(len(values) * 0.95)],
        }

    return {
        'num_runs': num_runs,
        'mean_total_ms': statistics.mean(results),
        'median_total_ms': statistics.median(results),
        'stdev_total_ms': statistics.stdev(results),
        'min_total_ms': min(results),
        'max_total_ms': max(results),
        'percentiles': percentile_vals,
        'components': component_stats,
        'raw_results': results,
    }


def print_report(profile: dict):
    """Pretty-print profiling results"""
    print("=" * 80)
    print("KOLBOT CHROME → AGENT LATENCY PROFILE (Monte Carlo, 10,000 runs)")
    print("=" * 80)
    print()

    print("SUMMARY STATISTICS (Total Latency)")
    print("-" * 80)
    print(f"  Mean:        {profile['mean_total_ms']:7.2f} ms")
    print(f"  Median:      {profile['median_total_ms']:7.2f} ms")
    print(f"  Stdev:       {profile['stdev_total_ms']:7.2f} ms")
    print(f"  Min:         {profile['min_total_ms']:7.2f} ms")
    print(f"  Max:         {profile['max_total_ms']:7.2f} ms")
    print()

    print("PERCENTILE DISTRIBUTION")
    print("-" * 80)
    for p in [1, 5, 10, 25, 50, 75, 90, 95, 99]:
        print(f"  P{p:2d}:        {profile['percentiles'][p]:7.2f} ms")
    print()

    print("COMPONENT BREAKDOWN (Mean ± Stdev, ms)")
    print("-" * 80)
    components = profile['components']
    for name, stats in components.items():
        label = name.replace('_', ' ').title()
        print(f"  {label:35s} {stats['mean']:6.2f} ± {stats['stdev']:5.2f} ms "
              f"(p95: {stats['p95']:6.2f}, range: {stats['min']:5.2f}–{stats['max']:5.2f})")
    print()

    # Identify bottleneck
    bottleneck = max(components.items(), key=lambda x: x[1]['mean'])
    print("CRITICAL PATH ANALYSIS")
    print("-" * 80)
    print(f"  Bottleneck:  {bottleneck[0].replace('_', ' ').title()}")
    print(f"  Impact:      {bottleneck[1]['mean'] / profile['mean_total_ms'] * 100:.1f}% of total latency")
    print()

    # Impact assessment
    print("IMPACT ASSESSMENT")
    print("-" * 80)
    mean_latency = profile['mean_total_ms']
    p95_latency = profile['percentiles'][95]
    game_tick_ms = 40  # Agent runs at 25 Hz = 40ms per tick

    print(f"  Agent tick interval:    {game_tick_ms} ms (25 Hz)")
    print(f"  Mean latency:           {mean_latency:.1f} ms ({mean_latency/game_tick_ms:.1%} of tick)")
    print(f"  P95 latency:            {p95_latency:.1f} ms ({p95_latency/game_tick_ms:.1%} of tick)")
    print()

    if mean_latency < 10:
        verdict = "✓ EXCELLENT — sub-tick latency, config applies in next frame"
    elif mean_latency < 20:
        verdict = "✓ GOOD — <50% tick, acceptable for gameplay"
    elif mean_latency < 40:
        verdict = "⚠ ACCEPTABLE — <1 tick, some observability but not critical"
    elif mean_latency < 80:
        verdict = "⚠ CONCERNING — ~1-2 ticks, user sees stale behavior"
    else:
        verdict = "✗ POOR — >2 ticks, noticeable delay to player"

    print(f"  Verdict:  {verdict}")
    print()

    # Latency histogram
    print("HISTOGRAM (Frequency Distribution)")
    print("-" * 80)
    buckets = 20
    bucket_size = (profile['max_total_ms'] - profile['min_total_ms']) / buckets
    buckets_list = [0] * buckets
    for val in profile['raw_results']:
        idx = min(int((val - profile['min_total_ms']) / bucket_size), buckets - 1)
        buckets_list[idx] += 1

    max_count = max(buckets_list)
    for i, count in enumerate(buckets_list):
        left = profile['min_total_ms'] + i * bucket_size
        right = left + bucket_size
        bar_width = int(50 * count / max_count)
        bar = '█' * bar_width
        pct = 100 * count / len(profile['raw_results'])
        print(f"  {left:6.1f}–{right:6.1f} ms │ {bar:50s} {pct:5.1f}%")
    print()


if __name__ == '__main__':
    # Standard config (5KB)
    print("\n### SCENARIO 1: Standard Config (5KB)\n")
    profile_std = monte_carlo_profile(num_runs=10000, config_size=5000)
    print_report(profile_std)

    # Large config (50KB) - unlikely but possible with complex farming setups
    print("\n### SCENARIO 2: Large Config (50KB)\n")
    profile_large = monte_carlo_profile(num_runs=10000, config_size=50000)
    print_report(profile_large)

    # Critical update: changing hp_potion_pct (small config fragment)
    print("\n### SCENARIO 3: Small Update (500B - single field)\n")
    profile_small = monte_carlo_profile(num_runs=10000, config_size=500)
    print_report(profile_small)

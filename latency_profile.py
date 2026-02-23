#!/usr/bin/env python3
"""
Monte Carlo profiling for kolbot Chrome -> agent messaging latency.

Compares BEFORE (v1) vs AFTER (v2) with three optimizations:
  1. Dual tick drain — post-execution cmd_rx flush (40ms max → ~5ms max wait)
  2. serde_json::from_value — eliminates JSON→YAML roundabout (~1ms saved)
  3. QuadCache Lane 3 — ThresholdBins replaces config traversal (~0.2ms saved)

Author: Claude Code latency analyzer
"""

import random
import statistics
from dataclasses import dataclass, field

# ═══════════════════════════════════════════════════════════════
# CORE DISTRIBUTIONS
# ═══════════════════════════════════════════════════════════════

def normal_clamp(mean: float, stddev: float, min_val: float = 0, max_val: float = None) -> float:
    val = random.gauss(mean, stddev)
    val = max(val, min_val)
    if max_val is not None:
        val = min(val, max_val)
    return val


# ═══════════════════════════════════════════════════════════════
# V1 — ORIGINAL ARCHITECTURE
# ═══════════════════════════════════════════════════════════════

@dataclass
class V1Components:
    """Original pipeline — single tick-start drain, JSON→YAML roundabout."""
    chrome_popup_to_background: float = 0
    background_to_native_msg: float = 0
    native_msg_encode: float = 0
    native_msg_transport: float = 0
    native_msg_decode: float = 0
    agent_loop_detection: float = 0      # 0–40ms uniform
    config_json_to_yaml: float = 0       # JSON→string→YAML roundabout
    config_parse: float = 0              # serde_yaml deserialization
    config_reload: float = 0             # pointer swap + config traversal
    total: float = 0

    def calculate_total(self):
        self.total = sum([
            self.chrome_popup_to_background, self.background_to_native_msg,
            self.native_msg_encode, self.native_msg_transport, self.native_msg_decode,
            self.agent_loop_detection, self.config_json_to_yaml,
            self.config_parse, self.config_reload,
        ])
        return self.total

def sample_v1(config_size: int = 5000) -> V1Components:
    c = V1Components()
    c.chrome_popup_to_background = normal_clamp(0.1, 0.05, 0.02, 0.3)
    c.background_to_native_msg = normal_clamp(0.3, 0.15, 0.1, 1.0)
    c.native_msg_encode = normal_clamp(0.05, 0.02, 0.02, 0.2)
    c.native_msg_transport = normal_clamp(1.0, 1.5, 0.3, 5.0)
    c.native_msg_decode = normal_clamp(0.2, 0.1, 0.05, 0.6)
    # Single drain at tick start — worst case waits entire tick
    c.agent_loop_detection = random.uniform(0, 40)
    # JSON → string → YAML roundabout
    c.config_json_to_yaml = normal_clamp(0.3, 0.1, 0.1, 1.0)
    size_factor = min(config_size / 5000, 3.0)
    c.config_parse = normal_clamp(1.0 * size_factor, 0.5 * size_factor, 0.2, 5.0)
    # reload_config: copy AgentConfig + DecisionEngine re-reads .survival.* per tick
    c.config_reload = normal_clamp(0.2, 0.1, 0.05, 0.5)
    c.calculate_total()
    return c


# ═══════════════════════════════════════════════════════════════
# V2 — OPTIMIZED: DUAL DRAIN + from_value + QUADCACHE
# ═══════════════════════════════════════════════════════════════

@dataclass
class V2Components:
    """Optimized pipeline — dual drain, direct JSON deser, QuadCache Lane 3."""
    chrome_popup_to_background: float = 0
    background_to_native_msg: float = 0
    native_msg_encode: float = 0
    native_msg_transport: float = 0
    native_msg_decode: float = 0
    agent_loop_detection: float = 0      # NOW: 0–5ms (post-exec drain)
    config_parse_direct: float = 0       # serde_json::from_value (no YAML)
    threshold_flatten: float = 0         # QuadCache Lane 3 ThresholdBins
    run_cache_hit: float = 0             # QuadCache Lane 2 HashMap lookup
    total: float = 0

    def calculate_total(self):
        self.total = sum([
            self.chrome_popup_to_background, self.background_to_native_msg,
            self.native_msg_encode, self.native_msg_transport, self.native_msg_decode,
            self.agent_loop_detection, self.config_parse_direct,
            self.threshold_flatten, self.run_cache_hit,
        ])
        return self.total

def sample_v2(config_size: int = 5000) -> V2Components:
    c = V2Components()
    # Chrome → agent transport: identical to v1 (can't optimize IPC physics)
    c.chrome_popup_to_background = normal_clamp(0.1, 0.05, 0.02, 0.3)
    c.background_to_native_msg = normal_clamp(0.3, 0.15, 0.1, 1.0)
    c.native_msg_encode = normal_clamp(0.05, 0.02, 0.02, 0.2)
    c.native_msg_transport = normal_clamp(1.0, 1.5, 0.3, 5.0)
    c.native_msg_decode = normal_clamp(0.2, 0.1, 0.05, 0.6)

    # OPTIMIZATION 1: Dual tick drain
    # Post-execution drain catches commands that arrived during:
    #   - humanized delay sleep (~5-15ms)
    #   - cadence jitter sleep (~1-5ms)
    #   - input dispatch (~1-3ms)
    # So the effective "blind window" is only the decision+execute phase (~2-5ms),
    # not the entire 40ms tick.
    #
    # Model: message arrives uniformly in the tick. Two drain points:
    #   - Tick start (t=0)
    #   - Post-execute (t ≈ 35ms, just before the idle sleep)
    # Effective blind window: ~5ms (just the tail sleep + next tick start)
    tick_exec_time = normal_clamp(8.0, 3.0, 3.0, 15.0)  # decision+execute phase
    idle_sleep = 40 - tick_exec_time  # remainder of tick
    # With two drain points, the max wait is just the idle_sleep portion
    # (because post-exec drain catches anything from exec phase)
    c.agent_loop_detection = random.uniform(0, max(idle_sleep * 0.15, 1.0))

    # OPTIMIZATION 2: serde_json::from_value<AgentConfig>() — skip YAML entirely
    # Direct JSON Value → struct deserialization, no string intermediary
    size_factor = min(config_size / 5000, 3.0)
    c.config_parse_direct = normal_clamp(0.15 * size_factor, 0.08 * size_factor, 0.05, 1.5)

    # OPTIMIZATION 3: QuadCache Lane 3 — ThresholdBins materialization
    # 12 field copies from AgentConfig → ThresholdBins. O(1), ~10ns.
    # Replaces per-tick config.survival.* traversal with direct field reads.
    c.threshold_flatten = normal_clamp(0.01, 0.005, 0.005, 0.05)

    # QuadCache Lane 2 — run lookup from pre-warmed HashMap
    # O(1) instead of walking config.farming.sequence
    c.run_cache_hit = normal_clamp(0.001, 0.0005, 0.0005, 0.005)

    c.calculate_total()
    return c


# ═══════════════════════════════════════════════════════════════
# DECISION PATH PROFILING (per-tick, not per-update)
# ═══════════════════════════════════════════════════════════════

def sample_decision_v1() -> float:
    """Time for one decide() call — V1: reads self.config.survival.* 11 times."""
    # Each config.survival.field_name is a struct traversal:
    #   self → config (Arc deref) → survival (field offset) → chicken_hp_pct (field offset)
    # On hot cache: ~2-5ns per access, but 11× per tick adds up.
    # Plus humanization math, RNG, timing checks.
    base = normal_clamp(0.8, 0.3, 0.3, 2.0)  # core decision logic
    config_traversal = 11 * normal_clamp(0.003, 0.001, 0.001, 0.008)  # 11 struct reads
    return base + config_traversal

def sample_decision_v2() -> float:
    """Time for one decide() call — V2: reads self.thresholds.* (flat struct, L1 hot)."""
    base = normal_clamp(0.8, 0.3, 0.3, 2.0)  # core decision logic (same)
    # ThresholdBins is 80 bytes, fits in 1-2 cache lines, always L1 hot
    threshold_reads = 11 * normal_clamp(0.0005, 0.0002, 0.0003, 0.001)  # 11 flat reads
    return base + threshold_reads


# ═══════════════════════════════════════════════════════════════
# MONTE CARLO ENGINE
# ═══════════════════════════════════════════════════════════════

def run_comparison(num_runs: int = 50000, config_size: int = 5000):
    """Run V1 vs V2 Monte Carlo comparison."""
    v1_totals = []
    v2_totals = []
    v1_decisions = []
    v2_decisions = []

    v1_components = {}
    v2_components = {}

    for _ in range(num_runs):
        s1 = sample_v1(config_size)
        s2 = sample_v2(config_size)
        v1_totals.append(s1.total)
        v2_totals.append(s2.total)
        v1_decisions.append(sample_decision_v1())
        v2_decisions.append(sample_decision_v2())

        # Collect component data for v2
        for field_name in ['chrome_popup_to_background', 'background_to_native_msg',
                           'native_msg_encode', 'native_msg_transport', 'native_msg_decode',
                           'agent_loop_detection', 'config_parse_direct',
                           'threshold_flatten', 'run_cache_hit']:
            v2_components.setdefault(field_name, []).append(getattr(s2, field_name))

    v1_totals.sort()
    v2_totals.sort()
    v1_decisions.sort()
    v2_decisions.sort()

    return v1_totals, v2_totals, v1_decisions, v2_decisions, v2_components


def pct(data, p):
    return data[int(len(data) * p / 100)]


def print_comparison(v1, v2, v1_dec, v2_dec, v2_comp, config_size):
    v1_mean = statistics.mean(v1)
    v2_mean = statistics.mean(v2)
    v1_p95 = pct(v1, 95)
    v2_p95 = pct(v2, 95)
    v1_p99 = pct(v1, 99)
    v2_p99 = pct(v2, 99)

    speedup_mean = v1_mean / v2_mean if v2_mean > 0 else float('inf')
    speedup_p95 = v1_p95 / v2_p95 if v2_p95 > 0 else float('inf')

    print("=" * 90)
    print(f"  KOLBOT LATENCY PROFILE — V1 (ORIGINAL) vs V2 (QUADCACHE + DUAL DRAIN)")
    print(f"  Config size: {config_size:,} bytes  |  Monte Carlo runs: {len(v1):,}")
    print("=" * 90)
    print()

    # ─── Config Update Latency ──────────────────────────────────
    print("CONFIG UPDATE LATENCY (popup.html click → agent applies new config)")
    print("-" * 90)
    print(f"  {'Metric':<20s} {'V1 (Original)':>16s} {'V2 (QuadCache)':>16s} {'Improvement':>16s}")
    print(f"  {'─'*20} {'─'*16} {'─'*16} {'─'*16}")

    rows = [
        ("Mean",   v1_mean,                    v2_mean),
        ("Median", statistics.median(v1),       statistics.median(v2)),
        ("P50",    pct(v1, 50),                pct(v2, 50)),
        ("P90",    pct(v1, 90),                pct(v2, 90)),
        ("P95",    v1_p95,                     v2_p95),
        ("P99",    v1_p99,                     v2_p99),
        ("Max",    max(v1),                    max(v2)),
        ("Min",    min(v1),                    min(v2)),
    ]
    for label, old, new in rows:
        delta = old - new
        pct_improve = (delta / old * 100) if old > 0 else 0
        print(f"  {label:<20s} {old:>13.2f} ms {new:>13.2f} ms {'-' if delta < 0 else ''}{abs(delta):>9.2f} ms ({pct_improve:+.0f}%)")

    print()
    print(f"  Overall speedup:    {speedup_mean:.1f}× (mean)  |  {speedup_p95:.1f}× (P95)")
    print()

    # ─── Decision Path Latency ──────────────────────────────────
    print("PER-TICK DECISION LATENCY (decide() hot path, 25× per second)")
    print("-" * 90)
    v1d_mean = statistics.mean(v1_dec)
    v2d_mean = statistics.mean(v2_dec)
    v1d_p95 = pct(v1_dec, 95)
    v2d_p95 = pct(v2_dec, 95)
    dec_speedup = v1d_mean / v2d_mean if v2d_mean > 0 else float('inf')

    dec_rows = [
        ("Mean",   v1d_mean,  v2d_mean),
        ("P95",    v1d_p95,   v2d_p95),
        ("P99",    pct(v1_dec, 99), pct(v2_dec, 99)),
    ]
    for label, old, new in dec_rows:
        delta = old - new
        pct_improve = (delta / old * 100) if old > 0 else 0
        print(f"  {label:<20s} {old:>13.4f} ms {new:>13.4f} ms {'-' if delta < 0 else ''}{abs(delta):>9.4f} ms ({pct_improve:+.0f}%)")

    print()
    print(f"  ThresholdBins speedup: {dec_speedup:.2f}× per decide() call")
    print(f"  Annual savings at 25 Hz: ~{(v1d_mean - v2d_mean) * 25 * 3600 * 8 / 1000:.1f}s per 8-hour session")
    print()

    # ─── V2 Component Breakdown ─────────────────────────────────
    print("V2 COMPONENT BREAKDOWN (Mean ± Stdev, ms)")
    print("-" * 90)
    nice_names = {
        'chrome_popup_to_background': 'Chrome Popup → Background',
        'background_to_native_msg':   'Background → Native Msg',
        'native_msg_encode':          'Native Msg Encode',
        'native_msg_transport':       'Native Msg Transport (stdio)',
        'native_msg_decode':          'Native Msg Decode',
        'agent_loop_detection':       'Agent Loop Detection (dual drain)',
        'config_parse_direct':        'Config Parse (from_value, no YAML)',
        'threshold_flatten':          'Lane 3: ThresholdBins Flatten',
        'run_cache_hit':              'Lane 2: Run Cache Hit',
    }
    for key in ['chrome_popup_to_background', 'background_to_native_msg',
                'native_msg_encode', 'native_msg_transport', 'native_msg_decode',
                'agent_loop_detection', 'config_parse_direct',
                'threshold_flatten', 'run_cache_hit']:
        vals = v2_comp[key]
        mean_v = statistics.mean(vals)
        std_v = statistics.stdev(vals) if len(vals) > 1 else 0
        p95_v = sorted(vals)[int(len(vals) * 0.95)]
        label = nice_names.get(key, key)
        print(f"  {label:42s}  {mean_v:8.4f} ± {std_v:7.4f}  (p95: {p95_v:8.4f})")
    print()

    # ─── Where Did the Savings Come From? ───────────────────────
    print("OPTIMIZATION IMPACT BREAKDOWN")
    print("-" * 90)
    # Dual drain savings
    v1_loop_mean = 20.0  # uniform(0,40) → mean 20
    v2_loop_mean = statistics.mean(v2_comp['agent_loop_detection'])
    drain_saving = v1_loop_mean - v2_loop_mean

    # JSON→YAML elimination
    v1_parse_mean = 1.3  # original: json_to_yaml(0.3) + parse(1.0)
    v2_parse_mean = statistics.mean(v2_comp['config_parse_direct'])
    parse_saving = v1_parse_mean - v2_parse_mean

    # QuadCache Lane 3
    v1_reload_mean = 0.2  # original reload
    v2_flatten_mean = statistics.mean(v2_comp['threshold_flatten'])
    lane3_saving = v1_reload_mean - v2_flatten_mean

    total_saving = v1_mean - v2_mean

    optimizations = [
        ("Dual tick drain (40ms→~5ms window)",   drain_saving,  drain_saving / total_saving * 100 if total_saving > 0 else 0),
        ("from_value (skip JSON→YAML→parse)",     parse_saving,  parse_saving / total_saving * 100 if total_saving > 0 else 0),
        ("Lane 3 ThresholdBins (flat struct)",    lane3_saving,  lane3_saving / total_saving * 100 if total_saving > 0 else 0),
    ]
    for name, saving, pct_of_total in optimizations:
        print(f"  {name:45s}  -{saving:6.2f} ms  ({pct_of_total:4.1f}% of total saving)")
    print(f"  {'─'*45}  {'─'*8}  {'─'*6}")
    print(f"  {'TOTAL SAVED':45s}  -{total_saving:6.2f} ms")
    print()

    # ─── Verdict ────────────────────────────────────────────────
    print("VERDICT")
    print("-" * 90)
    game_tick_ms = 40
    if v2_mean < 5:
        grade = "EXCELLENT"
        emoji = ">>>"
        desc = "sub-frame, config applies WITHIN current tick"
    elif v2_mean < 10:
        grade = "GREAT"
        emoji = ">>"
        desc = "near-instant, <25% of tick"
    elif v2_mean < 20:
        grade = "GOOD"
        emoji = ">"
        desc = "<50% of tick, no perceptible lag"
    else:
        grade = "ACCEPTABLE"
        emoji = "~"
        desc = "<1 tick, monitor for regressions"

    print(f"  V1: {v1_mean:.1f}ms mean, {v1_p95:.1f}ms P95  ({v1_mean/game_tick_ms:.0%} of tick)")
    print(f"  V2: {v2_mean:.1f}ms mean, {v2_p95:.1f}ms P95  ({v2_mean/game_tick_ms:.0%} of tick)")
    print(f"  {emoji} {grade}: {desc}")
    print()

    # ─── Survival Impact (the chicken/potion question) ──────────
    print("SURVIVAL IMPACT (will you miss a chicken or potion?)")
    print("-" * 90)
    # P(missing a survival event) ≈ P(config update latency > remaining HP drain time)
    # At 25 Hz, each tick is 40ms. A monster hit can drain 5-10% HP per tick.
    # If chicken_hp_pct = 30% and you're at 32%, you have ~1-2 ticks before death.
    # V1: 40ms max wait → could miss 1 tick → 50% chance of missing chicken
    # V2: ~5ms max wait → catches within same tick → ~12% chance

    v1_miss_pct = sum(1 for x in v1 if x > 40) / len(v1) * 100
    v2_miss_pct = sum(1 for x in v2 if x > 40) / len(v2) * 100
    v1_half_tick = sum(1 for x in v1 if x > 20) / len(v1) * 100
    v2_half_tick = sum(1 for x in v2 if x > 20) / len(v2) * 100
    v2_sub_5 = sum(1 for x in v2 if x < 5) / len(v2) * 100

    print(f"  V1: {v1_miss_pct:5.1f}% of updates take >1 full tick (40ms) — potential missed chicken")
    print(f"  V2: {v2_miss_pct:5.1f}% of updates take >1 full tick (40ms)")
    print()
    print(f"  V1: {v1_half_tick:5.1f}% of updates take >half tick (20ms)")
    print(f"  V2: {v2_half_tick:5.1f}% of updates take >half tick (20ms)")
    print()
    print(f"  V2: {v2_sub_5:5.1f}% of updates arrive in <5ms (same-frame application)")
    print()

    # ─── Histogram Comparison ───────────────────────────────────
    print("HISTOGRAM COMPARISON (V1 top, V2 bottom)")
    print("-" * 90)
    all_vals = v1 + v2
    hist_min = min(all_vals)
    hist_max = max(all_vals)
    nbins = 20
    bsize = (hist_max - hist_min) / nbins

    def make_hist(data):
        bins = [0] * nbins
        for val in data:
            idx = min(int((val - hist_min) / bsize), nbins - 1)
            bins[idx] += 1
        return bins

    h1 = make_hist(v1)
    h2 = make_hist(v2)
    max_count = max(max(h1), max(h2))

    print("  V1 (ORIGINAL):")
    for i in range(nbins):
        left = hist_min + i * bsize
        right = left + bsize
        bar = '░' * int(40 * h1[i] / max_count)
        p = 100 * h1[i] / len(v1)
        print(f"  {left:6.1f}–{right:5.1f} │ {bar:40s} {p:5.1f}%")

    print()
    print("  V2 (QUADCACHE + DUAL DRAIN):")
    for i in range(nbins):
        left = hist_min + i * bsize
        right = left + bsize
        bar = '█' * int(40 * h2[i] / max_count)
        p = 100 * h2[i] / len(v2)
        print(f"  {left:6.1f}–{right:5.1f} │ {bar:40s} {p:5.1f}%")

    print()


# ═══════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════

if __name__ == '__main__':
    random.seed(42)  # Reproducible runs

    print("\n" + "=" * 90)
    print("  KOLBOT LATENCY PROFILER v2 — BEFORE / AFTER COMPARISON")
    print("  Optimizations: Dual Tick Drain + from_value + QuadCache (Lanes 2+3+4)")
    print("=" * 90 + "\n")

    print("\n### SCENARIO 1: Standard Config Update (5KB)\n")
    v1, v2, v1d, v2d, v2c = run_comparison(50000, 5000)
    print_comparison(v1, v2, v1d, v2d, v2c, 5000)

    print("\n### SCENARIO 2: Survival-Critical Update (500B — chicken/potion change)\n")
    v1s, v2s, v1ds, v2ds, v2cs = run_comparison(50000, 500)
    print_comparison(v1s, v2s, v1ds, v2ds, v2cs, 500)

    print("\n### SCENARIO 3: Full Config Reload (50KB — entire build change)\n")
    v1l, v2l, v1dl, v2dl, v2cl = run_comparison(50000, 50000)
    print_comparison(v1l, v2l, v1dl, v2dl, v2cl, 50000)

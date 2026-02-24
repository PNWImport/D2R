#!/usr/bin/env python3
"""
D2R Map Helper — Full Verification Test Suite
==============================================
Tests the Chrome Native Messaging Host binary against every command,
validates response schemas, checks map generation determinism,
and measures latency.
"""

import struct
import json
import subprocess
import time
import sys
import os
from pathlib import Path

# Try rich for pretty output, fall back to plain
try:
    from rich.console import Console
    from rich.table import Table
    from rich.panel import Panel
    from rich import box
    console = Console()
    HAS_RICH = True
except ImportError:
    HAS_RICH = False
    class FakeConsole:
        def print(self, *a, **kw): print(*[str(x) for x in a])
        def rule(self, *a, **kw): print("=" * 60)
    console = FakeConsole()

# ---------------------------------------------------------------------------
# Native Messaging Protocol Helpers
# ---------------------------------------------------------------------------

class NativeMessagingClient:
    """Communicates with a Chrome Native Messaging Host process."""

    def __init__(self, binary_path: str):
        self.binary = binary_path
        self.proc = None
        self.responses = []
        self.latencies = []

    def start(self):
        self.proc = subprocess.Popen(
            [self.binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def send(self, msg: dict) -> None:
        data = json.dumps(msg).encode("utf-8")
        length = struct.pack("<I", len(data))
        self.proc.stdin.write(length + data)
        self.proc.stdin.flush()

    def recv(self, timeout: float = 2.0) -> list:
        """Read all available responses (non-blocking after timeout)."""
        results = []
        deadline = time.time() + timeout

        while time.time() < deadline:
            try:
                raw = self.proc.stdout.read1(65536)
                if not raw:
                    time.sleep(0.05)
                    continue
                idx = 0
                while idx < len(raw) - 4:
                    length = struct.unpack("<I", raw[idx:idx + 4])[0]
                    idx += 4
                    if idx + length > len(raw):
                        break
                    payload = json.loads(raw[idx:idx + length])
                    results.append(payload)
                    idx += length
                if results:
                    break
            except (BlockingIOError, OSError):
                time.sleep(0.05)

        self.responses.extend(results)
        return results

    def send_recv(self, msg: dict, timeout: float = 2.0) -> dict:
        """Send a command and return the first response."""
        t0 = time.monotonic()
        self.send(msg)
        results = self.recv(timeout)
        elapsed_ms = (time.monotonic() - t0) * 1000
        self.latencies.append(elapsed_ms)
        return results[0] if results else {}

    def stop(self):
        if self.proc:
            try:
                self.send({"cmd": "shutdown"})
                self.proc.wait(timeout=3)
            except Exception:
                self.proc.kill()


# ---------------------------------------------------------------------------
# Test Cases
# ---------------------------------------------------------------------------

class TestResult:
    def __init__(self, name: str, passed: bool, detail: str = "", latency_ms: float = 0):
        self.name = name
        self.passed = passed
        self.detail = detail
        self.latency_ms = latency_ms


def run_tests(binary_path: str) -> list:
    results = []
    client = NativeMessagingClient(binary_path)
    client.start()
    time.sleep(0.2)

    # ---- Test 1: Handshake ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "handshake", "version": "test-1.0"})
    lat = (time.monotonic() - t0) * 1000

    passed = (
        r.get("cmd") == "handshake_ack"
        and r.get("type") == "map_helper"
        and "version" in r
        and "pid" in r
        and r.get("offsets_version") == "kzb-compat-2026"
    )
    results.append(TestResult(
        "Handshake",
        passed,
        f"v{r.get('version')} PID={r.get('pid')} offsets={r.get('offsets_version')}",
        lat,
    ))

    # ---- Test 2: Ping/Pong ----
    ts = int(time.time() * 1000)
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "ping", "timestamp": ts})
    lat = (time.monotonic() - t0) * 1000

    passed = (
        r.get("cmd") == "pong"
        and r.get("timestamp") == ts
        and "server_time" in r
    )
    results.append(TestResult("Ping/Pong", passed, f"roundtrip echo OK", lat))

    # ---- Test 3: Read State (simulated) ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "read_state"})
    lat = (time.monotonic() - t0) * 1000

    gs = r.get("game_state", {})
    passed = (
        r.get("cmd") == "state"
        and r.get("d2r_attached") == True
        and gs.get("in_game") == True
        and gs.get("map_seed") is not None
        and gs.get("area_name") is not None
        and gs.get("player_x", 0) > 0
    )
    results.append(TestResult(
        "Read State",
        passed,
        f"seed={hex(gs.get('map_seed', 0))} area={gs.get('area_name')} "
        f"pos=({gs.get('player_x')},{gs.get('player_y')}) diff={gs.get('difficulty')}",
        lat,
    ))

    # ---- Test 4: Map has POIs and overlay data ----
    map_data = r.get("map", {})
    passed = (
        map_data is not None
        and map_data.get("poi_count", 0) > 0
        and map_data.get("width", 0) > 0
        and map_data.get("height", 0) > 0
    )
    results.append(TestResult(
        "Auto-Map Generation",
        passed,
        f"dims={map_data.get('width')}x{map_data.get('height')} "
        f"pois={map_data.get('poi_count')} rows={map_data.get('collision_rows')}",
        0,
    ))

    # ---- Test 5: Explicit Map Generation ----
    t0 = time.monotonic()
    r = client.send_recv({
        "cmd": "generate_map",
        "seed": 26396577,
        "area_id": 74,  # Arcane Sanctuary
        "difficulty": 2,
    })
    lat = (time.monotonic() - t0) * 1000

    passed = (
        r.get("cmd") == "map_data"
        and r.get("seed") == 26396577
        and r.get("area_id") == 74
        and r.get("difficulty") == 2
        and r.get("collision_row_count", 0) > 0
        and len(r.get("pois", [])) > 0
    )
    results.append(TestResult(
        "Generate Map (Arcane)",
        passed,
        f"seed=26396577 {r.get('width')}x{r.get('height')} "
        f"pois={len(r.get('pois',[]))} rows={r.get('collision_row_count')}",
        lat,
    ))

    # ---- Test 6: Determinism (same seed = same map) ----
    r2 = client.send_recv({
        "cmd": "generate_map",
        "seed": 26396577,
        "area_id": 74,
        "difficulty": 2,
    })

    passed = (
        r.get("collision_rows") == r2.get("collision_rows")
        and r.get("pois") == r2.get("pois")
        and r.get("origin_x") == r2.get("origin_x")
        and r.get("origin_y") == r2.get("origin_y")
    )
    results.append(TestResult(
        "Determinism Check",
        passed,
        "Same seed → identical collision + POIs" if passed else "MISMATCH!",
        0,
    ))

    # ---- Test 7: Different seed = different map ----
    r3 = client.send_recv({
        "cmd": "generate_map",
        "seed": 99999,
        "area_id": 74,
        "difficulty": 2,
    })

    passed = (
        r.get("collision_rows") != r3.get("collision_rows")
        or r.get("origin_x") != r3.get("origin_x")
    )
    results.append(TestResult(
        "Seed Variance",
        passed,
        "Different seeds → different maps" if passed else "Maps identical (bad!)",
        0,
    ))

    # ---- Test 8: Toggle Map ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "toggle_map", "enabled": False})
    lat = (time.monotonic() - t0) * 1000

    passed = r.get("cmd") == "toggle_ack" and r.get("enabled") == False
    results.append(TestResult("Toggle Off", passed, f"enabled={r.get('enabled')}", lat))

    # Verify disabled state
    r = client.send_recv({"cmd": "read_state"})
    passed = r.get("enabled") == False and r.get("in_game") == False
    results.append(TestResult(
        "Disabled Blocks Reads",
        passed,
        "read_state returns disabled when map off",
        0,
    ))

    # Re-enable
    client.send_recv({"cmd": "toggle_map", "enabled": True})

    # ---- Test 9: Set Opacity ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "set_opacity", "opacity": 128})
    lat = (time.monotonic() - t0) * 1000

    passed = r.get("cmd") == "opacity_ack" and r.get("opacity") == 128
    results.append(TestResult("Set Opacity", passed, f"opacity={r.get('opacity')}", lat))

    # ---- Test 10: Opacity clamping ----
    r = client.send_recv({"cmd": "set_opacity", "opacity": 5})
    passed = r.get("opacity") == 10  # min clamp
    results.append(TestResult("Opacity Clamp", passed, f"input=5 → clamped={r.get('opacity')}", 0))

    # ---- Test 11: Cache Stats ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "cache_stats"})
    lat = (time.monotonic() - t0) * 1000

    passed = (
        r.get("cmd") == "cache_stats"
        and r.get("cached_maps", 0) >= 2  # We generated at least 2 maps
        and r.get("max_cache") == 128
        and r.get("poll_count", 0) >= 1
    )
    results.append(TestResult(
        "Cache Stats",
        passed,
        f"cached={r.get('cached_maps')}/{r.get('max_cache')} polls={r.get('poll_count')}",
        lat,
    ))

    # ---- Test 12: Get Offsets ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "get_offsets"})
    lat = (time.monotonic() - t0) * 1000

    offsets = r.get("offsets", {})
    passed = (
        r.get("cmd") == "offsets"
        and offsets.get("player_hash_table") == 0x2028E60
        and offsets.get("ui_settings") == 0x20AD5F0
        and "unit_any" in offsets
        and "act_misc" in offsets
        and offsets.get("act_misc", {}).get("map_seed") == 0x840
    )
    results.append(TestResult(
        "Get Offsets",
        passed,
        f"hash_table={hex(offsets.get('player_hash_table', 0))} "
        f"seed_off={hex(offsets.get('act_misc', {}).get('map_seed', 0))}",
        lat,
    ))

    # ---- Test 13: Set Area (manual override) ----
    r = client.send_recv({"cmd": "set_area", "area": 108, "difficulty": 2})
    passed = r.get("cmd") == "area_ack" and r.get("area") == 108
    results.append(TestResult("Set Area Override", passed, f"area=108 (Chaos Sanctuary)", 0))

    # ---- Test 14: Unknown command ----
    r = client.send_recv({"cmd": "totally_bogus_command"})
    passed = r.get("cmd") == "error"
    results.append(TestResult("Unknown Command", passed, f"Returns error for bad cmd", 0))

    # ---- Test 15: Mass generation stress test ----
    areas = [2, 8, 37, 74, 108, 131, 128, 65, 12, 39]
    t0 = time.monotonic()
    for area in areas:
        client.send({"cmd": "generate_map", "seed": 1337, "area_id": area, "difficulty": 2})
    client.proc.stdin.flush()
    time.sleep(1.5)
    bulk = client.recv(2.0)
    lat = (time.monotonic() - t0) * 1000

    passed = len(bulk) >= len(areas)
    results.append(TestResult(
        f"Bulk Gen ({len(areas)} areas)",
        passed,
        f"Generated {len(bulk)} maps in {lat:.0f}ms ({lat/max(len(bulk),1):.1f}ms/map)",
        lat,
    ))

    # ---- Test 16: Shutdown ----
    t0 = time.monotonic()
    r = client.send_recv({"cmd": "shutdown"})
    lat = (time.monotonic() - t0) * 1000

    passed = r.get("cmd") == "shutdown_ack"
    results.append(TestResult("Shutdown", passed, "Clean exit", lat))

    try:
        client.proc.wait(timeout=3)
        exit_clean = True
    except subprocess.TimeoutExpired:
        client.proc.kill()
        exit_clean = False

    results.append(TestResult("Process Exit", exit_clean, f"returncode={client.proc.returncode}", 0))

    return results, client.latencies


# ---------------------------------------------------------------------------
# Pretty Output
# ---------------------------------------------------------------------------

def print_results(results: list, latencies: list):
    console.rule("[bold cyan]D2R Map Helper — Verification Report")
    console.print()

    if HAS_RICH:
        table = Table(title="Test Results", box=box.ROUNDED, show_lines=True)
        table.add_column("#", style="dim", width=3)
        table.add_column("Test", style="bold")
        table.add_column("Status", width=6)
        table.add_column("Detail")
        table.add_column("Latency", justify="right", width=10)

        for i, r in enumerate(results, 1):
            status = "[green]PASS[/]" if r.passed else "[red]FAIL[/]"
            lat_str = f"{r.latency_ms:.1f}ms" if r.latency_ms > 0 else "-"
            table.add_row(str(i), r.name, status, r.detail, lat_str)

        console.print(table)
    else:
        for i, r in enumerate(results, 1):
            status = "PASS" if r.passed else "FAIL"
            lat_str = f"{r.latency_ms:.1f}ms" if r.latency_ms > 0 else "-"
            console.print(f"  [{status}] {i:2d}. {r.name}: {r.detail} ({lat_str})")

    console.print()

    # Summary
    passed = sum(1 for r in results if r.passed)
    failed = sum(1 for r in results if not r.passed)
    total = len(results)

    timed = [l for l in latencies if l > 0]
    avg_lat = sum(timed) / len(timed) if timed else 0
    max_lat = max(timed) if timed else 0

    if HAS_RICH:
        summary = Table(box=box.SIMPLE)
        summary.add_column("Metric", style="bold")
        summary.add_column("Value", justify="right")
        summary.add_row("Tests Passed", f"[green]{passed}[/]/{total}")
        summary.add_row("Tests Failed", f"[red]{failed}[/]/{total}" if failed else f"[green]0[/]/{total}")
        summary.add_row("Avg Latency", f"{avg_lat:.1f}ms")
        summary.add_row("Max Latency", f"{max_lat:.1f}ms")
        summary.add_row("Binary Size", get_binary_size())

        color = "green" if failed == 0 else "red"
        verdict = "ALL TESTS PASSED" if failed == 0 else f"{failed} TEST(S) FAILED"
        console.print(Panel(summary, title=f"[bold {color}]{verdict}[/]", border_style=color))
    else:
        console.print(f"  Passed: {passed}/{total}  Failed: {failed}/{total}")
        console.print(f"  Avg Latency: {avg_lat:.1f}ms  Max: {max_lat:.1f}ms")
        console.print(f"  Binary: {get_binary_size()}")
        verdict = "ALL TESTS PASSED" if failed == 0 else f"{failed} TEST(S) FAILED"
        console.print(f"\n  >>> {verdict} <<<")


def get_binary_size() -> str:
    for path in [
        "target/release/chrome_map_helper",
        "target/debug/chrome_map_helper",
    ]:
        p = Path(path)
        if p.exists():
            size = p.stat().st_size
            if size > 1_048_576:
                return f"{size / 1_048_576:.1f} MB ({path})"
            else:
                return f"{size / 1024:.0f} KB ({path})"
    return "N/A"


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    # Find binary
    binary = None
    for candidate in [
        "target/release/chrome_map_helper",
        "target/debug/chrome_map_helper",
        "./chrome_map_helper",
    ]:
        if Path(candidate).exists():
            binary = candidate
            break

    if not binary:
        console.print("[red]ERROR: chrome_map_helper binary not found![/]")
        console.print("Run: cargo build --release")
        sys.exit(1)

    console.print(f"[dim]Binary: {binary}[/]")
    console.print(f"[dim]Time: {time.strftime('%Y-%m-%d %H:%M:%S')}[/]")
    console.print()

    results, latencies = run_tests(binary)
    print_results(results, latencies)

    failed = sum(1 for r in results if not r.passed)
    sys.exit(1 if failed > 0 else 0)

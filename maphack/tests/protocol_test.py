#!/usr/bin/env python3
"""
Native Messaging Protocol Test — maphack
Exercises the chrome_map_helper binary via stdin/stdout native messaging.
Works without Chrome or the game installed.

Usage:
    python3 tests/protocol_test.py [path_to_binary]
"""

import json
import struct
import subprocess
import sys
import time
import os

BINARY = None
PASS = 0
FAIL = 0
TOTAL = 0


def find_binary():
    """Find the chrome_map_helper binary."""
    candidates = [
        "./target/debug/chrome_map_helper",
        "./target/release/chrome_map_helper",
        "../maphack/target/debug/chrome_map_helper",
    ]
    for c in candidates:
        if os.path.exists(c):
            return c
    return None


def send_msg(proc, msg):
    """Send a native messaging formatted message."""
    data = json.dumps(msg).encode("utf-8")
    proc.stdin.write(struct.pack("<I", len(data)))
    proc.stdin.write(data)
    proc.stdin.flush()


def recv_msg(proc, timeout=3.0):
    """Receive a native messaging formatted response."""
    import select

    # Read 4-byte length prefix
    length_data = b""
    start = time.time()
    while len(length_data) < 4:
        if time.time() - start > timeout:
            return None
        chunk = proc.stdout.read(4 - len(length_data))
        if chunk:
            length_data += chunk
        else:
            time.sleep(0.01)

    msg_len = struct.unpack("<I", length_data)[0]
    if msg_len > 1_048_576:
        return None

    # Read message body
    body = b""
    while len(body) < msg_len:
        if time.time() - start > timeout:
            return None
        chunk = proc.stdout.read(msg_len - len(body))
        if chunk:
            body += chunk
        else:
            time.sleep(0.01)

    return json.loads(body)


def test(name, msg, check_fn):
    """Run a single protocol test."""
    global PASS, FAIL, TOTAL
    TOTAL += 1

    proc = subprocess.Popen(
        [BINARY],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    try:
        send_msg(proc, msg)
        resp = recv_msg(proc, timeout=3.0)

        if resp is None:
            print(f"  FAIL  {name} — no response (timeout)")
            FAIL += 1
            return

        ok, detail = check_fn(resp)
        if ok:
            print(f"  PASS  {name}")
            PASS += 1
        else:
            print(f"  FAIL  {name} — {detail}")
            print(f"         Response: {json.dumps(resp, indent=2)}")
            FAIL += 1

    except Exception as e:
        print(f"  FAIL  {name} — exception: {e}")
        FAIL += 1
    finally:
        proc.stdin.close()
        proc.terminate()
        proc.wait(timeout=2)


def main():
    global BINARY

    if len(sys.argv) > 1:
        BINARY = sys.argv[1]
    else:
        BINARY = find_binary()

    if not BINARY or not os.path.exists(BINARY):
        print("Binary not found. Build first: cargo build")
        print("Usage: python3 tests/protocol_test.py [path_to_binary]")
        sys.exit(1)

    print(f"\nKZB Map Helper — Protocol Test")
    print(f"Binary: {BINARY}")
    print(f"{'=' * 50}\n")

    # ── Test 1: Handshake ──
    test(
        "Handshake returns version and type",
        {"cmd": "handshake", "version": "1.4.0"},
        lambda r: (
            r.get("cmd") == "handshake_ack"
            and "version" in r
            and r.get("type") == "map_helper",
            f"cmd={r.get('cmd')}, type={r.get('type')}",
        ),
    )

    # ── Test 2: Ping/Pong ──
    ts = int(time.time() * 1000)
    test(
        "Ping returns pong with timestamp",
        {"cmd": "ping", "timestamp": ts},
        lambda r: (
            r.get("cmd") == "pong" and r.get("timestamp") == ts,
            f"cmd={r.get('cmd')}, ts={r.get('timestamp')}",
        ),
    )

    # ── Test 3: Read State (no game attached) ──
    test(
        "ReadState without game returns error",
        {"cmd": "read_state"},
        lambda r: (
            r.get("cmd") == "state",
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Test 4: Toggle Map ──
    test(
        "Toggle map returns ack",
        {"cmd": "toggle_map", "enabled": False},
        lambda r: (
            r.get("cmd") == "toggle_ack" and r.get("enabled") is False,
            f"cmd={r.get('cmd')}, enabled={r.get('enabled')}",
        ),
    )

    # ── Test 5: Set Opacity ──
    test(
        "Set opacity returns ack",
        {"cmd": "set_opacity", "opacity": 200},
        lambda r: (
            r.get("cmd") == "opacity_ack" and r.get("opacity") == 200,
            f"cmd={r.get('cmd')}, opacity={r.get('opacity')}",
        ),
    )

    # ── Test 6: Cache Stats ──
    test(
        "Cache stats returns counts",
        {"cmd": "cache_stats"},
        lambda r: (
            r.get("cmd") == "cache_stats" and "cached_maps" in r,
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Test 7: Get Offsets ──
    test(
        "Get offsets returns offset data",
        {"cmd": "get_offsets"},
        lambda r: (
            r.get("cmd") == "offsets" and "offsets" in r,
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Test 8: Activate Map ──
    test(
        "Activate map returns ack with duration",
        {"cmd": "activate_map", "duration_ms": 3000},
        lambda r: (
            r.get("cmd") == "activate_ack"
            and r.get("activated") is True
            and r.get("duration_ms") == 3000,
            f"cmd={r.get('cmd')}, duration={r.get('duration_ms')}",
        ),
    )

    # ── Test 9: Deactivate Map ──
    test(
        "Deactivate map returns ack",
        {"cmd": "deactivate_map"},
        lambda r: (
            r.get("cmd") == "deactivate_ack" and r.get("deactivated") is True,
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Test 10: Generate Map (mock) ──
    test(
        "Generate map returns map data",
        {"cmd": "generate_map", "seed": 12345, "area_id": 1, "difficulty": 0},
        lambda r: (
            r.get("cmd") == "map_data" and "width" in r and "height" in r,
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Test 11: No d2/D2R in wire responses ──
    def check_no_d2(r):
        text = json.dumps(r).lower()
        has_d2 = "d2r" in text or "d2vision" in text or "diablo" in text
        return (not has_d2, f"Found game reference in response: {text[:200]}")

    test(
        "Handshake response has no game-specific keywords",
        {"cmd": "handshake", "version": "1.4.0"},
        check_no_d2,
    )

    # ── Test 12: Unknown command returns error ──
    test(
        "Unknown command returns error",
        {"cmd": "totally_invalid_command"},
        lambda r: (
            r.get("cmd") == "error",
            f"cmd={r.get('cmd')}",
        ),
    )

    # ── Summary ──
    print(f"\n{'=' * 50}")
    print(f"Results: {PASS}/{TOTAL} passed, {FAIL} failed")

    if FAIL == 0:
        print("ALL TESTS PASSED")
    else:
        print(f"FAILURES: {FAIL}")

    sys.exit(0 if FAIL == 0 else 1)


if __name__ == "__main__":
    main()

"""Live stdio smoke test for midi-forge mcp --standalone."""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "dist" / "midi-forge.exe"
CREATE_NO_WINDOW = 0x08000000


def send(proc: subprocess.Popen, obj: dict) -> None:
    line = json.dumps(obj, separators=(",", ":")) + "\n"
    assert proc.stdin is not None
    proc.stdin.write(line.encode("utf-8"))
    proc.stdin.flush()


def read_msg(proc: subprocess.Popen, timeout: float = 8.0) -> dict:
    assert proc.stdout is not None
    deadline = time.time() + timeout
    buf = b""
    while time.time() < deadline:
        chunk = proc.stdout.read(1)
        if not chunk:
            err = proc.stderr.read() if proc.stderr else b""
            raise RuntimeError(
                f"EOF from MCP stdout. stderr={err!r} rc={proc.poll()}"
            )
        buf += chunk
        if buf.endswith(b"\n"):
            line = buf.decode("utf-8").strip()
            if not line:
                buf = b""
                continue
            return json.loads(line)
    raise TimeoutError(f"no JSON line in {timeout}s, buf={buf[:200]!r}")


def main() -> int:
    if not EXE.is_file():
        print(f"missing {EXE}", file=sys.stderr)
        return 1
    flags = CREATE_NO_WINDOW if sys.platform == "win32" else 0
    proc = subprocess.Popen(
        [str(EXE), "mcp", "--standalone"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=str(ROOT),
        creationflags=flags,
    )
    # Give tokio/rmcp a beat to bind stdio.
    time.sleep(0.2)
    try:
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "mcp-smoke", "version": "0.1"},
                },
            },
        )
        init = read_msg(proc)
        print("initialize:", json.dumps(init)[:800])
        if "error" in init:
            return 1
        send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized"})
        send(proc, {"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        listed = read_msg(proc)
        tools = [t["name"] for t in listed.get("result", {}).get("tools", [])]
        print("tools:", tools)
        expected = {
            "list_endpoints",
            "monitor_tail",
            "live_now",
            "clock_health",
            "stuck_notes",
            "thru_graph",
            "mpe_status",
            "snapshot",
            "send_note",
            "send_cc",
            "identity",
            "panic",
            "set_port_open",
        }
        missing = expected - set(tools)
        if missing:
            print("MISSING", missing)
            return 1
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "list_endpoints", "arguments": {}},
            },
        )
        eps = read_msg(proc)
        print("list_endpoints:", json.dumps(eps)[:1200])
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "send_note",
                    "arguments": {
                        "out": "synth",
                        "note": 60,
                        "vel": 100,
                        "ch": 1,
                        "group": 0,
                        "m2": False,
                    },
                },
            },
        )
        unarmed = read_msg(proc)
        print("send_note unarmed:", json.dumps(unarmed)[:800])
        text = json.dumps(unarmed).lower()
        if "arm" not in text:
            print("expected unarmed write to mention arm")
            return 1
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "snapshot", "arguments": {}},
            },
        )
        snap = read_msg(proc)
        print("snapshot:", json.dumps(snap)[:800])
        print("SMOKE OK")
        return 0
    finally:
        proc.kill()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            pass


if __name__ == "__main__":
    raise SystemExit(main())

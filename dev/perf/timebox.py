#!/usr/bin/env python3
"""Run a command for a bounded duration and terminate its process group."""

from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration", type=float, required=True)
    parser.add_argument("--signal", choices=["TERM", "INT"], default="TERM")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if args.command and args.command[0] == "--":
        args.command = args.command[1:]
    if not args.command:
        parser.error("missing command")

    return args


def main() -> int:
    args = parse_args()
    stop_signal = signal.SIGINT if args.signal == "INT" else signal.SIGTERM
    process = subprocess.Popen(args.command, start_new_session=True)

    try:
        return process.wait(timeout=args.duration)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, stop_signal)
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        return 124


if __name__ == "__main__":
    sys.exit(main())

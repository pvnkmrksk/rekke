#!/usr/bin/env python3
"""Receive Rekke UDP JSON using only Python's standard library."""
import argparse
import json
import socket

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--host", default="127.0.0.1")
parser.add_argument("--port", type=int, default=5601)
parser.add_argument("--count", type=int, default=0, help="Stop after N packets; 0 runs forever")
args = parser.parse_args()

def degrees(value):
    return "missing" if value is None else f"{value * 180 / 3.141592653589793:+.2f}°"

with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
    sock.bind((args.host, args.port))
    print(f"Listening on {args.host}:{args.port}", flush=True)
    count = 0
    try:
        while args.count == 0 or count < args.count:
            payload, peer = sock.recvfrom(65535)
            data = json.loads(payload)
            print(f"{data['instance']} #{data['sequence']} from {peer[0]}:{peer[1]}  "
                  f"L {degrees(data['left']['angle_rad'])}  R {degrees(data['right']['angle_rad'])}  "
                  f"L-R {degrees(data['left_minus_right_rad'])}", flush=True)
            count += 1
    except KeyboardInterrupt:
        pass

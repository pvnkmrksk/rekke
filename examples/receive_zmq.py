#!/usr/bin/env python3
"""Kinefly Docker-compatible receiver. Install pyzmq on the receiver only."""
import argparse
import json
import zmq

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--host", default="127.0.0.1")
parser.add_argument("--port", type=int, default=9871)
args = parser.parse_args()
context = zmq.Context()
socket = context.socket(zmq.SUB)
socket.connect(f"tcp://{args.host}:{args.port}")
socket.setsockopt(zmq.SUBSCRIBE, b"")
try:
    while True:
        data = json.loads(socket.recv_string())
        print(f"{data['frame_number']}  L={data['x']:.4f} R={data['y']:.4f} yaw={data['yaw']:.4f} t={data['timestamp_unix']:.6f}")
except KeyboardInterrupt:
    pass
finally:
    socket.close(linger=0)
    context.term()

#!/usr/bin/env python3
"""
Camera control script that runs with system-level packages.
Handles Picamera2 access that requires system-level dependencies.
"""
import sys
import os
import json
import argparse
import time
import io
import numpy as np
import cv2

os.environ["LIBCAMERA_LOG_FILE"] = "/dev/null"

from picamera2 import Picamera2
import libcamera


def bgr8_to_jpeg(value, quality=75):
    return bytes(cv2.imencode('.jpg', value, [cv2.IMWRITE_JPEG_QUALITY, quality])[1])


def init_camera():
    picam2 = Picamera2()
    config = picam2.create_preview_configuration(main={"format": "RGB888", "size": (320, 240)})
    config["transform"] = libcamera.Transform(hflip=1, vflip=1)
    picam2.configure(config)
    picam2.start()
    time.sleep(0.5)
    return picam2


def get_frame(picam2):
    if picam2 is None:
        return None
    try:
        return picam2.capture_array()
    except Exception:
        return None


def cmd_init(args):
    picam2 = init_camera()
    picam2.stop()
    picam2.close()
    print(json.dumps({"status": "ok"}))


def cmd_snapshot(args):
    picam2 = init_camera()
    frame = get_frame(picam2)
    picam2.stop()
    picam2.close()
    if frame is None:
        print(json.dumps({"error": "no frame"}), file=sys.stderr)
        sys.exit(1)
    jpeg = bgr8_to_jpeg(frame, args.quality)
    sys.stdout.buffer.write(jpeg)


def cmd_stream(args):
    picam2 = init_camera()
    try:
        while True:
            frame = get_frame(picam2)
            if frame is not None:
                jpeg = bgr8_to_jpeg(frame, args.quality)
                sys.stdout.buffer.write(jpeg)
                sys.stdout.buffer.flush()
            time.sleep(args.interval)
    except KeyboardInterrupt:
        pass
    finally:
        picam2.stop()
        picam2.close()


parser = argparse.ArgumentParser(description="Camera control")
subparsers = parser.add_subparsers(dest="command")

subparsers.add_parser("init")
p_snapshot = subparsers.add_parser("snapshot")
p_snapshot.add_argument("-q", "--quality", type=int, default=75)
p_stream = subparsers.add_parser("stream")
p_stream.add_argument("-q", "--quality", type=int, default=75)
p_stream.add_argument("-i", "--interval", type=float, default=0.05)

args = parser.parse_args()

if args.command == "init":
    cmd_init(args)
elif args.command == "snapshot":
    cmd_snapshot(args)
elif args.command == "stream":
    cmd_stream(args)
else:
    parser.print_help()

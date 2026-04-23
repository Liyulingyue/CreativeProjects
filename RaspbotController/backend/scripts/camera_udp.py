#!/usr/bin/env python3
"""
Camera UDP streaming server.
Runs with system Python (has picamera2).
Continuously broadcasts JPEG frames over UDP.
"""
import socket
import struct
import time
import os
import sys
import cv2
import numpy as np

os.environ["LIBCAMERA_LOG_FILE"] = "/dev/null"
os.environ["LIBCAMERA_LOG_LEVELS"] = "3"

from picamera2 import Picamera2
import libcamera

PICAM2_CONFIG = {"format": "RGB888", "size": (320, 240)}

HDR_MAGIC = b"FRM1"
HDR_FORMAT = 1


def make_frame_header(seq: int, size: int) -> bytes:
    return HDR_MAGIC + struct.pack("<BHI", HDR_FORMAT, seq, size)


def jpeg_encode(frame: np.ndarray, quality: int = 75) -> bytes:
    return cv2.imencode('.jpg', frame, [cv2.IMWRITE_JPEG_QUALITY, quality])[1].tobytes()


def main():
    host = "127.0.0.1"
    port = 9001
    quality = 75

    # Initialize Picamera2 with retries to handle transient init failures
    picam2 = None
    for attempt in range(1, 6):
        try:
            picam2 = Picamera2()
            config = picam2.create_preview_configuration(main=PICAM2_CONFIG)
            config["transform"] = libcamera.Transform(hflip=1, vflip=1)
            picam2.configure(config)
            picam2.start()
            time.sleep(1)
            break
        except Exception as e:
            print(f"[camera_udp] Picamera2 init failed (attempt {attempt}/5): {e}", flush=True)
            if attempt < 5:
                time.sleep(1)
            else:
                print("[camera_udp] Giving up Picamera2 init. Check camera connection, permissions, and dmesg for errors.", flush=True)
                sys.exit(1)

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"[camera_udp] Broadcasting on {host}:{port}", flush=True)
    seq = 0

    try:
        while True:
            frame = picam2.capture_array()
            jpeg = jpeg_encode(frame, quality)
            header = make_frame_header(seq, len(jpeg))
            packet = header + jpeg
            try:
                sock.sendto(packet, ("127.0.0.1", port))
                if seq % 30 == 0:
                    print(f"[camera_udp] Sent frame {seq}, size={len(jpeg)} bytes", flush=True)
            except Exception as e:
                print(f"[camera_udp] Send error: {e}", flush=True)
            seq = (seq + 1) % 65536
    except KeyboardInterrupt:
        pass
    finally:
        picam2.stop()
        picam2.close()
        sock.close()


if __name__ == "__main__":
    main()

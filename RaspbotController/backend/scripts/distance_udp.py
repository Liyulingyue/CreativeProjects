#!/usr/bin/env python3
"""
Distance sensor UDP server.
Runs with system Python (has RPi.GPIO).
Listens for distance measurement commands and returns readings.

Protocol:
  Receive:  DIS1 (4 bytes) + 1 byte command (0x01 = measure)
  Send:     DIS1 (4 bytes) + 4 bytes float (little-endian) = distance in cm
            or -1.0 on error/timeout
"""
import socket
import struct
import time
import sys

DIST_HOST = "0.0.0.0"
DIST_PORT = 9003
DIST_MAGIC = b"DIS1"
MEASURE_CMD = 0x01

GPIO = None
ECHO_PIN = 18
TRIG_PIN = 16


def init_gpio():
    global GPIO
    try:
        import RPi.GPIO as GPIO
        GPIO.setwarnings(False)
        GPIO.setmode(GPIO.BOARD)
        GPIO.setup(ECHO_PIN, GPIO.IN)
        GPIO.setup(TRIG_PIN, GPIO.OUT)
        print(f"[distance_udp] GPIO initialized, Trig={TRIG_PIN}, Echo={ECHO_PIN}", flush=True)
        return True
    except Exception as e:
        print(f"[distance_udp] GPIO init failed: {e}", flush=True)
        return False


def measure_distance() -> float:
    if not GPIO:
        return -1.0

    try:
        GPIO.output(TRIG_PIN, GPIO.LOW)
        time.sleep(0.000002)
        GPIO.output(TRIG_PIN, GPIO.HIGH)
        time.sleep(0.000015)
        GPIO.output(TRIG_PIN, GPIO.LOW)

        t3 = time.time()
        timeout_start = t3

        while not GPIO.input(ECHO_PIN):
            t4 = time.time()
            if (t4 - timeout_start) > 0.03:
                return -1.0

        t1 = time.time()
        timeout_start = t1

        while GPIO.input(ECHO_PIN):
            t5 = time.time()
            if (t5 - timeout_start) > 0.03:
                return -1.0

        t2 = time.time()
        time.sleep(0.01)

        distance = ((t2 - t1) * 340 / 2) * 100
        if distance < 0 or distance > 500:
            return -1.0

        return round(distance, 1)
    except Exception:
        return -1.0


def handle_command(data: bytes, sock: socket.socket, addr: tuple):
    if len(data) < 5 or data[:4] != DIST_MAGIC:
        return

    cmd = data[4]
    if cmd == MEASURE_CMD:
        distance = measure_distance()
        response = DIST_MAGIC + struct.pack("<f", distance)
        sock.sendto(response, addr)
        print(f"[distance_udp] Measure from {addr}: {distance} cm", flush=True)


def main():
    if not init_gpio():
        print("[distance_udp] Cannot start without GPIO, exiting.", flush=True)
        sys.exit(1)

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    try:
        sock.bind((DIST_HOST, DIST_PORT))
    except Exception as e:
        print(f"[distance_udp] bind failed: {e}", flush=True)
        sys.exit(1)

    print(f"[distance_udp] Listening on {DIST_HOST}:{DIST_PORT}", flush=True)

    try:
        while True:
            data, addr = sock.recvfrom(1024)
            handle_command(data, sock, addr)
    except KeyboardInterrupt:
        pass
    finally:
        if GPIO:
            GPIO.cleanup()
        sock.close()


if __name__ == "__main__":
    main()

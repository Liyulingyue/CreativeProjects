#!/usr/bin/env python3
"""
LED UDP controller server.
Runs with system Python (has RPi.GPIO).
Listens for LED commands over UDP and controls GPIO LEDs.

Protocol:
  Header: LED1 (4 bytes)
  Cmd:    1 byte
  Args:   varies

Commands:
  0x01 SET_ALL   - 1 byte: bitmask (bit0=LED1, bit1=LED2), 1=on, 0=off
  0x02 SET_LED1  - 1 byte: 1=on, 0=off
  0x03 SET_LED2  - 1 byte: 1=on, 0=off
  0x04 TOGGLE    - 1 byte: bitmask, toggles specified LEDs
  0x05 BLINK     - 2 bytes: bitmask + duration_ms (u16)
"""
import socket
import struct
import threading
import time
import sys

LED_HOST = "0.0.0.0"
LED_PORT = 9004
LED_MAGIC = b"LED1"

GPIO = None
LED1_PIN = 40
LED2_PIN = 38


def init_gpio():
    global GPIO
    try:
        import RPi.GPIO as GPIO
        GPIO.setwarnings(False)
        GPIO.setmode(GPIO.BOARD)
        GPIO.setup(LED1_PIN, GPIO.OUT)
        GPIO.setup(LED2_PIN, GPIO.OUT)
        GPIO.output(LED1_PIN, GPIO.HIGH)
        GPIO.output(LED2_PIN, GPIO.HIGH)
        print(f"[led_udp] GPIO initialized, LED1=Pin{LED1_PIN}, LED2=Pin{LED2_PIN} (HIGH=off)", flush=True)
        return True
    except Exception as e:
        print(f"[led_udp] GPIO init failed: {e}", flush=True)
        return False


def led_set(led_num: int, on: bool):
    if not GPIO:
        return
    pin = LED1_PIN if led_num == 1 else LED2_PIN
    GPIO.output(pin, GPIO.LOW if on else GPIO.HIGH)


def led_set_all(mask: int):
    led_set(1, bool(mask & 0x01))
    led_set(2, bool(mask & 0x02))


def handle_command(data: bytes) -> bool:
    print(f"[led_udp] received {len(data)} bytes: {data.hex()}", flush=True)
    if len(data) < 5 or data[:4] != LED_MAGIC:
        print(f"[led_udp] magic mismatch or too short", flush=True)
        return False

    cmd = data[4]
    if cmd == 0x01:
        if len(data) >= 6:
            led_set_all(data[5])
            print(f"[led_udp] SET_ALL mask={data[5]}", flush=True)
        return True
    elif cmd == 0x02:
        if len(data) >= 6:
            led_set(1, bool(data[5]))
            print(f"[led_udp] SET_LED1={data[5]}", flush=True)
        return True
    elif cmd == 0x03:
        if len(data) >= 6:
            led_set(2, bool(data[5]))
            print(f"[led_udp] SET_LED2={data[5]}", flush=True)
        return True
    elif cmd == 0x04:
        if len(data) >= 6:
            mask = data[5]
            threading.Thread(target=_blink_once, args=(mask, 100), daemon=True).start()
        return True
    elif cmd == 0x05:
        if len(data) >= 8:
            mask = data[5]
            duration = struct.unpack("<H", data[6:8])[0]
            threading.Thread(target=_blink_once, args=(mask, duration), daemon=True).start()
        return True
    return False


def _blink_once(mask: int, duration_ms: int):
    led_set_all(mask)
    time.sleep(duration_ms / 1000)
    led_set_all(0)


def main():
    if not init_gpio():
        print("[led_udp] Cannot start without GPIO, exiting.", flush=True)
        sys.exit(1)

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    try:
        sock.bind((LED_HOST, LED_PORT))
    except Exception as e:
        print(f"[led_udp] bind failed: {e}", flush=True)
        sys.exit(1)

    print(f"[led_udp] Listening on {LED_HOST}:{LED_PORT}", flush=True)

    try:
        while True:
            data, addr = sock.recvfrom(1024)
            if handle_command(data):
                print(f"[led_udp] Command from {addr}: processed", flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        if GPIO:
            GPIO.output(LED1_PIN, GPIO.HIGH)
            GPIO.output(LED2_PIN, GPIO.HIGH)
            GPIO.cleanup()
        sock.close()


if __name__ == "__main__":
    main()

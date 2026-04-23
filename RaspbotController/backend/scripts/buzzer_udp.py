#!/usr/bin/env python3
"""
Buzzer UDP controller server.
Runs with system Python (has RPi.GPIO).
Listens for buzzer commands over UDP and controls GPIO buzzer.

Protocol (all little-endian):
  Header: BUZ1 (4 bytes)
  Cmd:    1 byte
  Args:   varies by command

Commands:
  0x01 ON        - Start continuous beep at current frequency
  0x02 OFF       - Stop beeping
  0x03 BEEP      - Short beep, duration in ms (2 bytes, default 200)
  0x04 SET_FREQ - Set frequency, freq in Hz (4 bytes, default 440)
                  Changes current PWM frequency immediately
"""
import socket
import struct
import threading
import time
import sys

BUZZER_HOST = "0.0.0.0"
BUZZER_PORT = 9002
BUZZER_MAGIC = b"BUZ1"

GPIO = None
BUZZER_PIN = 32
buzzer_pwm = None
current_freq = 440


def init_gpio():
    global GPIO, buzzer_pwm, current_freq
    try:
        import RPi.GPIO as GPIO
        GPIO.setwarnings(False)
        GPIO.setmode(GPIO.BOARD)
        GPIO.setup(BUZZER_PIN, GPIO.OUT)
        buzzer_pwm = GPIO.PWM(BUZZER_PIN, current_freq)
        buzzer_pwm.start(0)
        print(f"[buzzer_udp] GPIO initialized, buzzer on pin {BUZZER_PIN}, freq={current_freq}Hz", flush=True)
        return True
    except Exception as e:
        print(f"[buzzer_udp] GPIO init failed: {e}", flush=True)
        return False


def beep_on():
    global buzzer_pwm
    if buzzer_pwm:
        buzzer_pwm.ChangeDutyCycle(50)


def beep_off():
    global buzzer_pwm
    if buzzer_pwm:
        buzzer_pwm.ChangeDutyCycle(0)


def beep_beep(duration_ms: int = 200):
    global buzzer_pwm
    if buzzer_pwm:
        buzzer_pwm.ChangeDutyCycle(50)
        time.sleep(duration_ms / 1000)
        buzzer_pwm.ChangeDutyCycle(0)


def set_freq(freq: int):
    global buzzer_pwm, current_freq
    freq = max(20, min(2000, freq))
    current_freq = freq
    if buzzer_pwm:
        buzzer_pwm.ChangeFrequency(freq)
    print(f"[buzzer_udp] Frequency set to {freq}Hz", flush=True)


def handle_command(data: bytes) -> bool:
    if len(data) < 5 or data[:4] != BUZZER_MAGIC:
        return False

    cmd = data[4]
    if cmd == 0x01:
        beep_on()
        return True
    elif cmd == 0x02:
        beep_off()
        return True
    elif cmd == 0x03:
        duration = struct.unpack("<H", data[5:7])[0] if len(data) >= 7 else 200
        threading.Thread(target=beep_beep, args=(duration,), daemon=True).start()
        return True
    elif cmd == 0x04:
        if len(data) >= 9:
            freq = struct.unpack("<I", data[5:9])[0]
            set_freq(freq)
            return True
        return False
    return False


def main():
    if not init_gpio():
        print("[buzzer_udp] Cannot start without GPIO, exiting.", flush=True)
        sys.exit(1)

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    try:
        sock.bind((BUZZER_HOST, BUZZER_PORT))
    except Exception as e:
        print(f"[buzzer_udp] bind failed: {e}", flush=True)
        sys.exit(1)

    print(f"[buzzer_udp] Listening on {BUZZER_HOST}:{BUZZER_PORT}", flush=True)

    try:
        while True:
            data, addr = sock.recvfrom(1024)
            if handle_command(data):
                print(f"[buzzer_udp] Command from {addr}: processed", flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        if buzzer_pwm:
            buzzer_pwm.stop()
        if GPIO:
            GPIO.cleanup()
        sock.close()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Hardware control script that runs with system-level packages.
Handles GPIO/I2C access that may require system-level dependencies.
"""
import sys
import json
import argparse
from typing import List

def get_hardware_device():
    try:
        from smbus2 import SMBus
        return SMBus(1)
    except ImportError:
        print(json.dumps({"error": "smbus not available"}))
        sys.exit(1)

DEVICE = None

def init_car():
    global DEVICE
    DEVICE = get_hardware_device()
    DEVICE.write_byte_data(0x16, 0x02, 0x00)

def ctrl_car(l_dir: int, l_speed: int, r_dir: int, r_speed: int):
    if DEVICE is None:
        init_car()
    reg = 0x01
    data = [l_dir, l_speed, r_dir, r_speed]
    DEVICE.write_i2c_block_data(0x16, reg, data)

def car_stop():
    if DEVICE is None:
        init_car()
    DEVICE.write_byte_data(0x16, 0x02, 0x00)

def car_run(speed1: int, speed2: int):
    ctrl_car(1, speed1, 1, speed2)

def car_back(speed1: int, speed2: int):
    ctrl_car(0, speed1, 0, speed2)

def car_left(speed1: int, speed2: int):
    ctrl_car(0, speed1, 1, speed2)

def car_right(speed1: int, speed2: int):
    ctrl_car(1, speed1, 0, speed2)

def car_spin_left(speed1: int, speed2: int):
    ctrl_car(0, speed1, 1, speed2)

def car_spin_right(speed1: int, speed2: int):
    ctrl_car(1, speed1, 0, speed2)

def ctrl_servo(servo_id: int, angle: int):
    if DEVICE is None:
        init_car()
    reg = 0x03
    angle = max(0, min(180, angle))
    DEVICE.write_i2c_block_data(0x16, reg, [servo_id, angle])

COMMANDS = {
    "init": lambda: init_car() or print(json.dumps({"status": "ok"})),
    "stop": lambda: car_stop() or print(json.dumps({"status": "ok"})),
    "run": lambda s1, s2: car_run(s1, s2) or print(json.dumps({"status": "ok"})),
    "back": lambda s1, s2: car_back(s1, s2) or print(json.dumps({"status": "ok"})),
    "left": lambda s1, s2: car_left(s1, s2) or print(json.dumps({"status": "ok"})),
    "right": lambda s1, s2: car_right(s1, s2) or print(json.dumps({"status": "ok"})),
    "spin_left": lambda s1, s2: car_spin_left(s1, s2) or print(json.dumps({"status": "ok"})),
    "spin_right": lambda s1, s2: car_spin_right(s1, s2) or print(json.dumps({"status": "ok"})),
    "servo": lambda sid, ang: ctrl_servo(sid, ang) or print(json.dumps({"status": "ok"})),
    "control": lambda l_dir, l_s, r_dir, r_s: ctrl_car(l_dir, l_s, r_dir, r_s) or print(json.dumps({"status": "ok"})),
}

def main():
    parser = argparse.ArgumentParser(description="Hardware control")
    parser.add_argument("command", choices=list(COMMANDS.keys()))
    parser.add_argument("args", nargs="*", type=int)
    args = parser.parse_args()

    cmd = COMMANDS[args.command]
    if args.args:
        cmd(*args.args)
    else:
        cmd()

if __name__ == "__main__":
    main()

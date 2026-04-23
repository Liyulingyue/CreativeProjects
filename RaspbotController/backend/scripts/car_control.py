#!/usr/bin/env python3
import sys
import json
import argparse

sys.path.insert(0, "app")

_car = None


def get_car():
    global _car
    if _car is None:
        from app.hardware.car import YB_Pcb_Car
        _car = YB_Pcb_Car()
    return _car


COMMANDS = {
    "init": lambda: get_car() or print(json.dumps({"status": "ok"})),
    "stop": lambda: get_car().Car_Stop() or print(json.dumps({"status": "ok"})),
    "run": lambda s1, s2: get_car().Car_Run(s1, s2) or print(json.dumps({"status": "ok"})),
    "back": lambda s1, s2: get_car().Car_Back(s1, s2) or print(json.dumps({"status": "ok"})),
    "left": lambda s1, s2: get_car().Car_Left(s1, s2) or print(json.dumps({"status": "ok"})),
    "right": lambda s1, s2: get_car().Car_Right(s1, s2) or print(json.dumps({"status": "ok"})),
    "servo": lambda sid, ang: get_car().Ctrl_Servo(sid, ang) or print(json.dumps({"status": "ok"})),
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

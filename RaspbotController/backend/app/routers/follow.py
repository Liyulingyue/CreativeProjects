from fastapi import APIRouter, HTTPException
import threading
import time
import socket
import struct
import json
import os

router = APIRouter(prefix="/api/v1/follow", tags=["Follow"])

CONFIG_FILE = "/home/pi/Codes/CreativeProjects/RaspbotController/backend/config/follow.json"
DIST_HOST = "127.0.0.1"
DIST_PORT = 9003
DIST_MAGIC = b"DIS1"

CONFIG_DEFAULTS = {
    "zone_near": 20,
    "zone_hold_max": 50,
    "zone_approach": 100,
    "zone_far": 120,
    "speed": 35,
    "scan_interval": 3
}

_follow_thread: threading.Thread | None = None
_follow_stop = threading.Event()
_follow_running = False
_follow_lock = threading.Lock()
_follow_state = {"zone": "IDLE", "distance": -1}


def load_config():
    if os.path.exists(CONFIG_FILE):
        try:
            with open(CONFIG_FILE) as f:
                return {**CONFIG_DEFAULTS, **json.load(f)}
        except Exception:
            pass
    return dict(CONFIG_DEFAULTS)


def udp_distance():
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(1.0)
        sock.sendto(DIST_MAGIC + bytes([0x01]), (DIST_HOST, DIST_PORT))
        data, _ = sock.recvfrom(64)
        sock.close()
        if len(data) >= 8 and data[:4] == DIST_MAGIC:
            d = struct.unpack("<f", data[4:8])[0]
            return d if 0 < d < 500 else -1
    except Exception:
        pass
    return -1


_car = None


def get_car():
    global _car
    if _car is None:
        from ..hardware.car import YB_Pcb_Car
        _car = YB_Pcb_Car()
    return _car


def get_zone(d, cfg):
    if d < 0:
        return "LOST"
    if d > cfg["zone_far"]:
        return "FAR"
    if d > cfg["zone_approach"]:
        return "APPROACH"
    if d >= cfg["zone_near"]:
        return "HOLD"
    return "NEAR"


def follow_loop():
    global _follow_running, _follow_state
    lost_count = 0

    while not _follow_stop.is_set():
        cfg = load_config()
        car = get_car()
        d = udp_distance()
        z = get_zone(d, cfg)

        with _follow_lock:
            _follow_state = {"zone": z, "distance": d}
            _follow_running = True

        if z == "FAR" or z == "LOST":
            lost_count += 1
            if lost_count >= cfg["scan_interval"]:
                lost_count = 0
                for method, s in [("Car_Left", 20), None, ("Car_Right", 20), None]:
                    if _follow_stop.is_set():
                        break
                    if method:
                        getattr(car, method)(s, s)
                    else:
                        car.Car_Stop()
                    time.sleep(0.3)
                    d2 = udp_distance()
                    if d2 > 0:
                        d = d2
                        z = get_zone(d, cfg)
                        with _follow_lock:
                            _follow_state = {"zone": z, "distance": d}
                        car.Car_Stop()
                        break
        else:
            lost_count = 0

        if z == "NEAR":
            car.Car_Back(cfg["speed"], cfg["speed"])
        elif z == "APPROACH":
            car.Car_Run(cfg["speed"], cfg["speed"])
        elif z == "HOLD":
            car.Car_Stop()
        elif z == "FAR":
            car.Car_Run(25, 25)
        else:
            car.Car_Stop()

        time.sleep(0.3)

    car.Car_Stop()
    with _follow_lock:
        _follow_state = {"zone": "IDLE", "distance": -1}
        _follow_running = False


@router.post("/start")
async def follow_start():
    global _follow_thread, _follow_stop, _follow_running
    with _follow_lock:
        if _follow_running:
            return {"status": "running"}
    if _follow_thread and _follow_thread.is_alive():
        return {"status": "running"}
    _follow_stop.clear()
    _follow_thread = threading.Thread(target=follow_loop, daemon=True)
    _follow_thread.start()
    return {"status": "started"}


@router.post("/stop")
async def follow_stop():
    global _follow_running
    _follow_stop.set()
    get_car().Car_Stop()
    return {"status": "stopped"}


@router.get("/status")
async def follow_status():
    with _follow_lock:
        return {"status": "running" if _follow_running else "stopped", **_follow_state}


@router.get("/config")
async def follow_config():
    return load_config()


@router.put("/config")
async def follow_config_update(cfg: dict):
    try:
        for key in cfg:
            if key not in CONFIG_DEFAULTS:
                raise ValueError(f"Unknown key: {key}")
        os.makedirs(os.path.dirname(CONFIG_FILE), exist_ok=True)
        with open(CONFIG_FILE, "w") as f:
            json.dump({**CONFIG_DEFAULTS, **cfg}, f, indent=2)
        return load_config()
    except Exception as e:
        raise HTTPException(400, str(e))

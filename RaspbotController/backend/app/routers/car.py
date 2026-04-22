from fastapi import APIRouter, HTTPException
from ..schemas import (
    MoveRequest,
    MoveResponse,
    StopResponse,
    ManualControlRequest,
    ManualControlResponse,
    ServoControlRequest,
    ServoControlResponse,
    BuzzerRequest,
    BuzzerResponse,
)
from ..hardware.car import YB_Pcb_Car
import socket
import struct

BUZZER_HOST = "127.0.0.1"
BUZZER_PORT = 9002
BUZZER_MAGIC = b"BUZ1"

router = APIRouter(prefix="/api/v1/car", tags=["Car"])

_car: YB_Pcb_Car | None = None


def get_car() -> YB_Pcb_Car:
    global _car
    if _car is None:
        _car = YB_Pcb_Car()
    return _car


DIRECTION_MAP = {
    "forward": "Car_Run",
    "backward": "Car_Back",
    "left": "Car_Left",
    "right": "Car_Right",
    "spin_left": "Car_Spin_Left",
    "spin_right": "Car_Spin_Right",
}


@router.post("/move", response_model=MoveResponse)
async def move(req: MoveRequest):
    if req.direction not in DIRECTION_MAP:
        raise HTTPException(status_code=400, detail=f"Invalid direction: {req.direction}")

    speed = max(0, min(100, req.speed))
    car = get_car()
    method_name = DIRECTION_MAP[req.direction]

    if req.direction == "stop":
        car.Car_Stop()
    else:
        getattr(car, method_name)(speed, speed)

    return MoveResponse(status="ok", direction=req.direction, speed=speed)


@router.post("/stop", response_model=StopResponse)
async def stop():
    get_car().Car_Stop()
    return StopResponse(status="ok")


@router.post("/manual", response_model=ManualControlResponse)
async def manual(req: ManualControlRequest):
    l_speed = max(-100, min(100, req.l_speed))
    r_speed = max(-100, min(100, req.r_speed))
    get_car().Control_Car(l_speed, r_speed)
    return ManualControlResponse(status="ok", l_speed=l_speed, r_speed=r_speed)


@router.post("/servo", response_model=ServoControlResponse)
async def servo(req: ServoControlRequest):
    get_car().Ctrl_Servo(req.id, req.angle)
    return ServoControlResponse(status="ok", id=req.id, angle=req.angle)


@router.post("/buzzer", response_model=BuzzerResponse)
async def buzzer(req: BuzzerRequest):
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        if req.action == "on":
            sock.sendto(BUZZER_MAGIC + bytes([0x01]), (BUZZER_HOST, BUZZER_PORT))
        elif req.action == "off":
            sock.sendto(BUZZER_MAGIC + bytes([0x02]), (BUZZER_HOST, BUZZER_PORT))
        elif req.action == "beep":
            sock.sendto(BUZZER_MAGIC + bytes([0x03]) + struct.pack("<H", 200), (BUZZER_HOST, BUZZER_PORT))
        elif req.action == "set_freq":
            sock.sendto(BUZZER_MAGIC + bytes([0x04]) + struct.pack("<I", req.frequency), (BUZZER_HOST, BUZZER_PORT))
        else:
            sock.close()
            raise HTTPException(status_code=400, detail=f"Invalid action: {req.action}")
        sock.close()
    except Exception as e:
        print(f"[buzzer] UDP error: {e}")
    return BuzzerResponse(status="ok", action=req.action, frequency=req.frequency)


DIST_HOST = "127.0.0.1"
DIST_PORT = 9003
DIST_MAGIC = b"DIS1"


@router.get("/distance", response_model=dict)
async def get_distance():
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(1.0)
        sock.sendto(DIST_MAGIC + bytes([0x01]), (DIST_HOST, DIST_PORT))
        data, _ = sock.recvfrom(64)
        sock.close()
        if len(data) >= 8 and data[:4] == DIST_MAGIC:
            distance = struct.unpack("<f", data[4:8])[0]
            return {"status": "ok", "distance": distance}
        return {"status": "error", "distance": -1}
    except Exception as e:
        print(f"[distance] UDP error: {e}")
        return {"status": "error", "distance": -1}


LED_HOST = "127.0.0.1"
LED_PORT = 9004
LED_MAGIC = b"LED1"


@router.post("/led", response_model=dict)
async def control_led(action: str, led: int = 1):
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        if action == "on":
            packet = LED_MAGIC + bytes([0x02] if led == 1 else [0x03]) + bytes([1])
        elif action == "off":
            packet = LED_MAGIC + bytes([0x02] if led == 1 else [0x03]) + bytes([0])
        elif action == "toggle":
            packet = LED_MAGIC + bytes([0x04]) + bytes([1 << (led - 1)])
        elif action == "blink":
            packet = LED_MAGIC + bytes([0x05]) + bytes([1 << (led - 1)]) + struct.pack("<H", 300)
        else:
            sock.close()
            raise HTTPException(status_code=400, detail=f"Invalid action: {action}")
        print(f"[led] sending {action} led{led}: {packet.hex()}", flush=True)
        sock.sendto(packet, (LED_HOST, LED_PORT))
        sock.close()
    except Exception as e:
        print(f"[led] UDP error: {e}")
    return {"status": "ok", "action": action, "led": led}

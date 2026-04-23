from fastapi import APIRouter, HTTPException
from ..schemas import MoveRequest, MoveResponse, StopResponse, ManualControlRequest, ManualControlResponse, ServoControlRequest, ServoControlResponse
from ..hardware.car import YB_Pcb_Car

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

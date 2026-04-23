from fastapi import APIRouter, HTTPException
from pydantic import BaseModel
import socket

router = APIRouter(prefix="/api/v1/led", tags=["LED"])

LED_HOST = "127.0.0.1"
LED_PORT = 9004
LED_MAGIC = b"LED1"


class LEDRequest(BaseModel):
    action: str
    led: int = 1


@router.post("/", response_model=dict)
async def led(req: LEDRequest):
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        if req.action == "on":
            packet = LED_MAGIC + bytes([0x02] if req.led == 1 else [0x03]) + bytes([1])
        elif req.action == "off":
            packet = LED_MAGIC + bytes([0x02] if req.led == 1 else [0x03]) + bytes([0])
        elif req.action == "toggle":
            packet = LED_MAGIC + bytes([0x04]) + bytes([1 << (req.led - 1)])
        elif req.action == "blink":
            packet = LED_MAGIC + bytes([0x05]) + bytes([1 << (req.led - 1)]) + bytes([0x2C, 0x01])
        else:
            sock.close()
            raise HTTPException(status_code=400, detail=f"Invalid action: {req.action}")
        sock.sendto(packet, (LED_HOST, LED_PORT))
        sock.close()
    except Exception as e:
        print(f"[led] UDP error: {e}")
    return {"status": "ok", "action": req.action, "led": req.led}

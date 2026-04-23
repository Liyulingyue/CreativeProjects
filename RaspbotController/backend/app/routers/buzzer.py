from fastapi import APIRouter, HTTPException
from ..schemas import BuzzerRequest, BuzzerResponse
import socket
import struct

router = APIRouter(prefix="/api/v1/buzzer", tags=["Buzzer"])

BUZZER_HOST = "127.0.0.1"
BUZZER_PORT = 9002
BUZZER_MAGIC = b"BUZ1"


@router.post("/", response_model=BuzzerResponse)
async def buzzer(req: BuzzerRequest):
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        if req.action == "on":
            packet = BUZZER_MAGIC + bytes([0x01])
        elif req.action == "off":
            packet = BUZZER_MAGIC + bytes([0x02])
        elif req.action == "beep":
            packet = BUZZER_MAGIC + bytes([0x03]) + struct.pack("<H", 200)
        elif req.action == "set_freq":
            packet = BUZZER_MAGIC + bytes([0x04]) + struct.pack("<I", req.frequency or 440)
        else:
            sock.close()
            raise HTTPException(status_code=400, detail=f"Invalid action: {req.action}")
        sock.sendto(packet, (BUZZER_HOST, BUZZER_PORT))
        sock.close()
    except Exception as e:
        print(f"[buzzer] UDP error: {e}")
    return BuzzerResponse(
        status="ok",
        action=req.action,
        frequency=req.frequency
    )

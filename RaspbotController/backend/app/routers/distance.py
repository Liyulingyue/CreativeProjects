from fastapi import APIRouter
import socket
import struct

router = APIRouter(prefix="/api/v1/distance", tags=["Distance"])

DIST_HOST = "127.0.0.1"
DIST_PORT = 9003
DIST_MAGIC = b"DIS1"


@router.get("/", response_model=dict)
async def distance():
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(1.0)
        sock.sendto(DIST_MAGIC + bytes([0x01]), (DIST_HOST, DIST_PORT))
        data, _ = sock.recvfrom(64)
        sock.close()
        if len(data) >= 8 and data[:4] == DIST_MAGIC:
            d = struct.unpack("<f", data[4:8])[0]
            return {"status": "ok", "distance": d}
        return {"status": "error", "distance": -1}
    except Exception as e:
        print(f"[distance] UDP error: {e}")
        return {"status": "error", "distance": -1}

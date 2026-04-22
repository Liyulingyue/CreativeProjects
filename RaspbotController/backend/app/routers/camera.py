from fastapi import APIRouter, HTTPException
from fastapi.responses import Response, StreamingResponse
import threading
import queue
import socket
import struct

router = APIRouter(prefix="/api/v1/camera", tags=["Camera"])

UDP_HOST = "0.0.0.0"
UDP_PORT = 9001
HDR_MAGIC = b"FRM1"

_frame_queue: queue.Queue[bytes] = queue.Queue(maxsize=2)
_recv_thread: threading.Thread | None = None
_recv_sock: socket.socket | None = None


def _udp_recv_loop():
    global _recv_sock
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    # Increase kernel receive buffer for UDP packets
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 512 * 1024)
    except Exception:
        pass
        
    try:
        sock.bind(("0.0.0.0", UDP_PORT))
    except Exception as e:
        print(f"[camera] thread: bind failed: {e}")
        sock.bind(("127.0.0.1", UDP_PORT))
        
    _recv_sock = sock
    print(f"[camera] thread: bound to port {UDP_PORT}")

    recv_count = 0
    while True:
        try:
            # Receive with a timeout to actually allow loop to see interrupts if needed
            # but here it's a daemon thread
            data, addr = sock.recvfrom(65535)
            if len(data) > 11 and data[:4] == HDR_MAGIC:
                fmt, seq, size = struct.unpack("<BHI", data[4:11])
                if fmt == 1 and size > 0 and len(data) >= 11 + size:
                    jpeg = data[11:11 + size] # Fixed offset is 11? HDR_MAGIC(4) + B(1) + H(2) + I(4) = 11
                    recv_count += 1
                    if recv_count % 20 == 0:
                        print(f"[camera] thread: received frame {seq} from {addr}, size={size}")
                    try:
                        _frame_queue.put_nowait(jpeg)
                    except queue.Full:
                        try:
                            _frame_queue.get_nowait()
                            _frame_queue.put_nowait(jpeg)
                        except queue.Empty:
                            pass
            else:
                if recv_count < 5:
                    print(f"[camera] thread: received invalid magic from {addr}: {data[:4]}")
        except Exception as e:
            print(f"[camera] thread error: {e}")
            time.sleep(0.1)


def _ensure_recv():
    global _recv_thread
    if _recv_thread is None or not _recv_thread.is_alive():
        _recv_thread = threading.Thread(target=_udp_recv_loop, daemon=True)
        _recv_thread.start()
        print("[camera] UDP receive thread started")


@router.get("/snapshot")
async def snapshot():
    _ensure_recv()
    print(f"[camera] snapshot: queue size={_frame_queue.qsize()}, thread alive={_recv_thread.is_alive() if _recv_thread else False}")
    try:
        jpeg = _frame_queue.get(timeout=3)
        print(f"[camera] snapshot: got frame {len(jpeg)} bytes")
        return Response(content=jpeg, media_type="image/jpeg")
    except queue.Empty:
        print("[camera] snapshot: queue empty")
        raise HTTPException(status_code=503, detail="No camera frame available")


@router.get("/stream")
async def stream():
    _ensure_recv()

    boundary = b"--frame"

    async def gen():
        last_frame = b""
        while True:
            try:
                jpeg = _frame_queue.get(timeout=5)
                if jpeg != last_frame:
                    last_frame = jpeg
                    header = b"\r\n" + boundary + b"\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n" % len(jpeg)
                    yield header + jpeg
            except queue.Empty:
                pass

    return StreamingResponse(gen(), media_type="multipart/x-mixed-replace; boundary=frame")

import uvicorn
import argparse
import os

def main():
    parser = argparse.ArgumentParser(description="OhMyDashboard Backend Runtime")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind")
    parser.add_argument("--port", type=int, default=8000, help="Port to listen on")
    parser.add_argument("--reload", action="store_true", help="Enable auto-reload for development")
    
    args = parser.parse_args()

    print(f">>> Starting OhMyDashboard Backend from app.main on http://{args.host}:{args.port}")
    
    # 指向 app.main:app
    uvicorn.run(
        "app.main:app",
        host=args.host,
        port=args.port,
        reload=args.reload
    )

if __name__ == "__main__":
    main()

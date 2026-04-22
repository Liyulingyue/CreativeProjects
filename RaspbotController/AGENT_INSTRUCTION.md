1. /home/pi/Yahboom_project/Raspbot/ 目录下有大量 Jupyter Notebook 文件用于交互小车
2. backend采用 fastapi 框架，提供 HTTP API 接口供前端调用
3. 前端采用 React 框架，提供用户界面供用户操作小车
4. 如果条件支持，使用 .venv 虚拟环境来管理 Python 依赖，确保环境隔离和依赖一致性,可能有一些和硬件相关的库需要在系统环境中安装，例如 GPIO 库等
5. 硬件控制通过 subprocess 调用系统级 Python 脚本实现（car_control.py、camera_control.py），因为 Picamera2 等库只在系统 Python 中可用
6. API: /health, /car/move, /car/stop, /car/manual, /servo/control, /camera/stream, /camera/snapshot
7. 项目结构: app/ (FastAPI 应用), backend/ (启动脚本), frontend/ (React), run.py (根目录启动)

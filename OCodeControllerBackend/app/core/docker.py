import logging
import secrets
import string
import subprocess

from app.config import settings

logger = logging.getLogger(__name__)


def _docker_command(args: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
    cmd = ["docker"] + args
    logger.debug(f"[Docker] Running: {' '.join(cmd)}")
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        if result.returncode != 0:
            logger.error(f"[Docker] Command failed: {' '.join(cmd)}")
            logger.error(f"[Docker] stdout: {result.stdout}")
            logger.error(f"[Docker] stderr: {result.stderr}")
        return result
    except subprocess.TimeoutExpired:
        logger.error(f"[Docker] Command timed out: {' '.join(cmd)}")
        raise


def _generate_password(length: int = 16) -> str:
    chars = string.ascii_letters + string.digits
    return "".join(secrets.choice(chars) for _ in range(length))


def generate_credentials() -> dict:
    return {
        "opencode_username": _generate_password(8),
        "opencode_password": _generate_password(16),
        "fb_username": _generate_password(8),
        "fb_password": _generate_password(16),
    }


def container_name(container_id: str) -> str:
    return f"opencode-{container_id}"


def is_running(container_id: str) -> bool:
    result = _docker_command(["ps", "--filter", f"name={container_name(container_id)}", "--format", "{{.Names}}"])
    return container_name(container_id) in result.stdout


def start_container(
    container_id: str,
    port: int,
    credentials: dict,
) -> str:
    name = container_name(container_id)
    if is_running(container_id):
        logger.info(f"[Docker] Container {name} already running")
        return name

    cmd = [
        "run", "-d",
        "--name", name,
        "-p", f"{port}:4096",
        "-p", f"{port + 1}:8080",
        "-e", f"OPENCODE_SERVER_USERNAME={credentials['opencode_username']}",
        "-e", f"OPENCODE_SERVER_PASSWORD={credentials['opencode_password']}",
        "-e", f"FB_USERNAME={credentials['fb_username']}",
        "-e", f"FB_PASSWORD={credentials['fb_password']}",
        settings.DOCKER_IMAGE,
    ]

    result = _docker_command(cmd)
    if result.returncode != 0:
        raise RuntimeError(f"Failed to start container: {result.stderr}")

    docker_container_id = result.stdout.strip()
    logger.info(f"[Docker] Started {name} (docker id: {docker_container_id})")
    return docker_container_id


def stop_container(container_id: str) -> None:
    name = container_name(container_id)
    if not is_running(container_id):
        logger.info(f"[Docker] Container {name} not running, nothing to stop")
        return

    _docker_command(["stop", name], timeout=30)
    _docker_command(["rm", name], timeout=30)
    logger.info(f"[Docker] Stopped and removed {name}")


def remove_container(container_id: str) -> None:
    name = container_name(container_id)
    _docker_command(["rm", "-f", name], timeout=30)
    logger.info(f"[Docker] Removed {name}")

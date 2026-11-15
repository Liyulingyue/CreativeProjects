"""Sandboxed shell execution for the `run_command` tool.

A command runner is only as safe as its boundary. The model reads text
that a stranger may have written (a file name, a note, a web page), so
"the model decided to run it" is not a safety argument. The boundary
has to come from somewhere else.

So commands run inside a bubblewrap sandbox:

  - the file system is read-only except the storage root
  - the storage root's manager directory (databases, auth, settings) is
    replaced by an empty tmpfs, so a command cannot read the API keys
    or rewrite the index
  - the network is unshared: no calls out
  - the environment is cleared, so nothing leaks through $VAR
  - skill resources are mounted read-only at /skills, so a skill can
    bring a script and have it run without the script being able to
    rewrite the skill
  - a wall-clock timeout and a CPU limit bound the damage

If bwrap is missing, nothing runs. There is deliberately no "just run
it directly" fallback: an unsandboxed shell on a family NAS is not a
reasonable default, and a tool that silently loses its boundary when a
package is missing is a tool nobody can reason about.
"""

from __future__ import annotations

import os
import resource
import shutil
import subprocess
import tempfile
from pathlib import Path

DEFAULT_TIMEOUT = 30
MIN_TIMEOUT = 1
MAX_TIMEOUT = 120
MAX_COMMAND_CHARS = 8000
MAX_OUTPUT_CHARS = 64 * 1024
MAX_FILE_BYTES = 4 * 1024 * 1024 * 1024  # 4 GB: stops a runaway fill, not a real job
# How much memory a command may take. Without it, one command could
# take the whole machine: this box has 30 GB of RAM and 8 GB of swap,
# and a python that asks for 20 GB of it takes the server down with it
# -- the killer does not know that the backend is more important than
# the command. The limit is on the address space, which is what the
# kernel can refuse; 4 GB is plenty for real work (numpy reserves half
# a gigabyte of address space before it has done anything).
MAX_MEMORY_BYTES = 4 * 1024 * 1024 * 1024
BWRAP = shutil.which("bwrap")

# Read-only system directories. Anything not listed here does not exist
# inside the sandbox, which is what keeps ~/.ssh, ~/.config and the
# application source out of reach.
SYSTEM_DIRS = ("/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc")

SANDBOX_SKILLS_DIR = "/skills"
SANDBOX_VENV_DIR = "/venv"


class SandboxError(RuntimeError):
    """The command could not be run (bad request, or no sandbox)."""


def sandbox_available() -> bool:
    return BWRAP is not None


def _within(path: Path, parent: Path) -> bool:
    try:
        path.resolve().relative_to(parent.resolve())
        return True
    except (ValueError, OSError):
        return False


def resolve_cwd(storage_root: Path, data_dir: Path, cwd: str) -> Path:
    """Resolve a working directory for a command.

    The directory must exist inside the storage root and must not be
    the manager directory, which the sandbox hides anyway.
    """
    raw = (cwd or "").strip()
    if raw.startswith("/") or raw.startswith("~"):
        raise SandboxError("工作目录必须是相对存储根目录的路径")
    if raw:
        target = (storage_root / raw).resolve()
        if not _within(target, storage_root):
            raise SandboxError(f"工作目录超出存储根目录: {raw}")
    else:
        target = storage_root.resolve()
    if not target.is_dir():
        raise SandboxError(f"工作目录不存在: {raw or '.'}")
    if _within(target, data_dir):
        raise SandboxError("不能在系统管理目录中执行命令")
    return target


def _preexec(timeout: int):
    """Apply the limits to the child before it execs."""
    def apply() -> None:
        os.setsid()
        cpu = timeout + 5
        try:
            resource.setrlimit(resource.RLIMIT_CPU, (cpu, cpu))
        except (ValueError, OSError):
            pass
        try:
            resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_FILE_BYTES, MAX_FILE_BYTES))
        except (ValueError, OSError):
            pass
        try:
            resource.setrlimit(resource.RLIMIT_AS, (MAX_MEMORY_BYTES, MAX_MEMORY_BYTES))
        except (ValueError, OSError):
            pass
    return apply


def _read_capped(path: Path) -> tuple[str, bool]:
    try:
        size = path.stat().st_size
    except OSError:
        return "", False
    try:
        with path.open("rb") as f:
            data = f.read(MAX_OUTPUT_CHARS)
    except OSError:
        return "", False
    text = data.decode("utf-8", "replace")
    return text, size > MAX_OUTPUT_CHARS


def build_args(
    storage_root: Path,
    data_dir: Path,
    skills_dir: Path,
    cwd: Path,
    venv: Path | None,
    command: str,
) -> list[str]:
    """Assemble the bwrap command line."""
    args = [
        BWRAP,
        "--die-with-parent",
        # Not --new-session: bwrap calls setsid() itself and fails when it
        # is already a session leader. The child gets its own session from
        # the preexec hook instead, which is also what lets a timeout kill
        # the whole group.
        "--unshare-net",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--clearenv",
    ]
    for src in SYSTEM_DIRS:
        if Path(src).exists():
            args += ["--ro-bind", src, src]
    args += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]
    # The storage root is the only writable place.
    args += ["--bind", str(storage_root), str(storage_root)]
    # The manager directory (databases, auth, settings, skills) is hidden
    # behind an empty tmpfs, so a command cannot read the API keys or
    # rewrite the index.
    if data_dir.exists() and _within(data_dir, storage_root):
        args += ["--tmpfs", str(data_dir)]
    if skills_dir.exists():
        args += ["--ro-bind", str(skills_dir), SANDBOX_SKILLS_DIR]
    if venv and venv.exists():
        args += ["--ro-bind", str(venv), SANDBOX_VENV_DIR]
    args += ["--chdir", str(cwd)]
    path = f"{SANDBOX_VENV_DIR}/bin:/usr/local/bin:/usr/bin:/bin" if venv else "/usr/local/bin:/usr/bin:/bin"
    args += [
        "--setenv", "PATH", path,
        "--setenv", "HOME", "/tmp",
        "--setenv", "TMPDIR", "/tmp",
        "--setenv", "LANG", "C.UTF-8",
        "--setenv", "LC_ALL", "C.UTF-8",
        "--setenv", "SKILLS_DIR", SANDBOX_SKILLS_DIR,
        "--setenv", "PYTHONUNBUFFERED", "1",
    ]
    args += ["bash", "-c", command]
    return args


def run(command: str, cwd: Path, timeout: int = DEFAULT_TIMEOUT) -> dict:
    """Run a command inside the sandbox and return its output.

    Raises SandboxError when the command could not be started at all.
    A non-zero exit code is not an error here: it is information the
    caller (and the model) needs to see.
    """
    if not BWRAP:
        raise SandboxError("沙箱不可用：系统缺少 bwrap，已拒绝执行命令")
    command = (command or "").strip()
    if not command:
        raise SandboxError("命令不能为空")
    if len(command) > MAX_COMMAND_CHARS:
        raise SandboxError(f"命令过长（上限 {MAX_COMMAND_CHARS} 字符）")
    timeout = max(MIN_TIMEOUT, min(int(timeout or DEFAULT_TIMEOUT), MAX_TIMEOUT))

    from .deps import DATA_DIR, get_storage_root

    storage_root = get_storage_root().resolve()
    data_dir = DATA_DIR.resolve()
    skills_dir = (data_dir / "skills")
    venv = Path(__file__).resolve().parents[1] / ".venv"

    args = build_args(storage_root, data_dir, skills_dir, cwd, venv, command)

    with tempfile.TemporaryDirectory(prefix="sfm-cmd-") as tmp:
        out_path = Path(tmp) / "stdout"
        err_path = Path(tmp) / "stderr"
        timed_out = False
        with out_path.open("wb") as out, err_path.open("wb") as err:
            proc = subprocess.Popen(
                args,
                stdin=subprocess.DEVNULL,
                stdout=out,
                stderr=err,
                cwd=str(cwd),
                preexec_fn=_preexec(timeout),
            )
            try:
                proc.wait(timeout=timeout + 5)
            except subprocess.TimeoutExpired:
                timed_out = True
                try:
                    os.killpg(proc.pid, 9)
                except (ProcessLookupError, PermissionError):
                    proc.kill()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    pass
        stdout, out_truncated = _read_capped(out_path)
        stderr, err_truncated = _read_capped(err_path)

    return {
        "returncode": proc.returncode if proc.returncode is not None else -1,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": timed_out,
        "truncated": out_truncated or err_truncated,
        "cwd": str(cwd),
        "command": command,
    }

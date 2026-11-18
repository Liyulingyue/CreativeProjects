#!/usr/bin/env python3
"""Put a backup back.

The backup script writes one archive holding the whole storage root.
This is the other half: it puts that archive back where the service
expects to find it.

Four things it does that unpacking the tar by hand does not:

  * it checks the archive first -- a backup that cannot be restored is
    refused rather than unpacked into a broken state;
  * it takes the extra top-level directory off, so the files land in
    the storage root itself and not one level too deep in a directory
    the service never looks at;
  * it moves the current data aside instead of overwriting it, so a
    restore that turns out to be the wrong one can be undone;
  * it refuses to work while the service is running, because the
    service writes to the very files being put back.

Usage:
    scripts/restore.py <archive> [target]

The target defaults to the storage root. Restore into an empty
directory first if you want to look before you leap:

    scripts/restore.py /mnt/backup/sfm/sfm-20261118-120000.tar.gz /tmp/check

Environment:
    SOURCE_DIR   the storage root (default: the backend's data directory)
    SFM_API_URL  the running service, to check whether it is up

Options:
    --force      restore even if the archive has problems, or the
                 service is running
    --dry-run    say what would happen, and change nothing
"""

import os
import shutil
import sys
import tarfile
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from archive_check import verify_archive  # noqa: E402  (a script, not a package)

PROJECT_DIR = Path(__file__).resolve().parent.parent
DEFAULT_TARGET = Path(os.getenv("SOURCE_DIR", PROJECT_DIR / "backend" / "data"))
API_URL = os.getenv("SFM_API_URL", "http://127.0.0.1:8000")


def service_is_running(url: str | None = None) -> bool:
    """Is the service up? A restore underneath a running service is a
    race: it writes to the same files that are being put back."""
    url = url or API_URL
    try:
        with urllib.request.urlopen(url.rstrip("/") + "/api/health/live", timeout=3) as resp:
            return resp.status == 200
    except urllib.error.HTTPError as e:
        # It answered, so it is running even if the answer was unhappy.
        return True
    except Exception:
        return False


def _common_root(names: list[str]) -> str | None:
    """The one directory every member sits under, if there is one."""
    tops = {n.split("/")[0] for n in names if n and not n.startswith("/")}
    if len(tops) == 1:
        return tops.pop()
    return None


def extract_archive(archive: Path, target: Path, dry_run: bool = False) -> int:
    """Unpack the archive into the target, without the extra level.

    Returns the number of files put back.
    """
    target.mkdir(parents=True, exist_ok=True)
    root = target.resolve()
    count = 0
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        names = [m.name for m in members]
        top = _common_root(names)
        if top:
            print(f"[restore] 归档顶层 : {top}/（会去掉这一层）")
        for member in members:
            parts = [p for p in member.name.split("/") if p not in ("", ".")]
            if top and parts and parts[0] == top:
                parts = parts[1:]
            if not parts:
                continue
            if ".." in parts:
                raise ValueError(f"归档里有跑出去的路径：{member.name}")
            out = (root / Path(*parts))
            try:
                out.resolve().relative_to(root)
            except ValueError:
                raise ValueError(f"归档里有跑出去的路径：{member.name}")
            if member.isdir():
                if not dry_run:
                    out.mkdir(parents=True, exist_ok=True)
                continue
            if not member.isfile():
                # A link or something else. Putting it back could point
                # somewhere outside the storage root, so it is left out
                # and said so.
                print(f"[restore] 跳过   : {member.name}（不是普通文件）")
                continue
            count += 1
            if dry_run:
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as src, open(out, "wb") as dst:
                shutil.copyfileobj(src, dst)
    return count


def main(argv: list[str]) -> int:
    args = [a for a in argv if not a.startswith("--")]
    flags = {a for a in argv if a.startswith("--")}
    force = "--force" in flags
    dry_run = "--dry-run" in flags

    if not args:
        print(__doc__)
        return 2
    archive = Path(args[0])
    target = Path(args[1]) if len(args) > 1 else DEFAULT_TARGET

    if not archive.is_file():
        print(f"还原失败：找不到归档 {archive}", file=sys.stderr)
        return 1

    print(f"[restore] 归档   : {archive}")
    print(f"[restore] 目标   : {target}")

    report = verify_archive(archive)
    for note in report.notes:
        print(f"[restore] 没查   : {note}")
    if not report.ok and not force:
        for problem in report.problems:
            print(f"还原失败：{problem}", file=sys.stderr)
        print("还原失败：这个归档有问题，先别用它还原（确实要用，加 --force）", file=sys.stderr)
        return 1
    if not report.ok:
        for problem in report.problems:
            print(f"[restore] 有问题 : {problem}（仍然还原，因为加了 --force）")

    # The service writes to the storage root. Restoring into a
    # scratch directory to look at the archive first is harmless even
    # while it is running; restoring over the live root is not.
    restoring_the_live_root = False
    try:
        restoring_the_live_root = target.resolve() == DEFAULT_TARGET.resolve()
    except OSError:
        pass
    if restoring_the_live_root and service_is_running():
        if not force:
            print(
                "还原失败：服务还在跑，它一边写、一边被还原，数据会打架。"
                "先停掉服务，再用 --force 继续。",
                file=sys.stderr,
            )
            return 1
        print("[restore] 服务   : 还在跑（仍然还原，因为加了 --force）")
    elif service_is_running():
        print("[restore] 服务   : 在跑，但这次不碰它用的目录，没关系")

    if target.exists() and any(target.iterdir()):
        stamp = time.strftime("%Y%m%d-%H%M%S")
        aside = target.with_name(target.name + f".before-restore-{stamp}")
        print(f"[restore] 旧数据 : 挪到 {aside}（不删）")
        if not dry_run:
            target.rename(aside)

    try:
        count = extract_archive(archive, target, dry_run=dry_run)
    except (tarfile.TarError, OSError, ValueError) as e:
        print(f"还原失败：{e}", file=sys.stderr)
        return 1

    if dry_run:
        print(f"[restore] 演练   : 会还原 {count} 个文件（什么都没改）")
    else:
        print(f"[restore] 完成   : 还原了 {count} 个文件")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

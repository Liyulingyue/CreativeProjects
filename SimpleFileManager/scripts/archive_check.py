#!/usr/bin/env python3
"""Can this archive really be put back?

A backup that cannot be restored is not a backup. It is a file that
looks like one, and the first time anyone finds out is the day they
need it. The backup script used to check only that the archive could be
opened and its names listed, which says nothing about whether the
databases inside open, or whether the vector store is a dataset that
loads. Both the backup and the restore script ask this module, so the
answer is the same either way.

The checks are deliberately narrow and cheap:

  * every database is opened and put through SQLite's own integrity
    check -- a half-copied file is caught here, not a year later;
  * every vector dataset is loaded and its rows counted -- a directory
    copied half way through a write does not open;
  * every member is actually read, not merely listed -- a truncated
    archive fails here instead of half way through a restore.

One thing that cannot be checked is said out loud: the vector store is
checked only if the script is running where lancedb can be imported.
The backend's environment has it; a bare system Python may not. In
that case the check is skipped and reported, never passed over in
silence -- "we did not look" and "we looked and it was fine" are not
the same statement.
"""

import shutil
import sys
import sqlite3
import tarfile
import tempfile
from pathlib import Path

# A Lance dataset is a directory whose name ends in ".lance". Its
# parent is the database it belongs to, and that is what gets opened.
LANCE_SUFFIX = ".lance"

# Side files of a SQLite database. The snapshot already contains
# everything they hold, so they are not part of the archive.
SQLITE_SIDE_SUFFIXES = ("-wal", "-shm", "-journal")


def _database_dirs(names: list[str]) -> set[str]:
    """The directories that hold Lance tables, and where they sit.

    Only the outermost ".lance" counts. A table is a directory, and
    the files inside it have names of their own -- "0.lance" among
    them -- and treating those as tables in their own right would send
    the checker looking for databases that do not exist.
    """
    dirs = set()
    for name in names:
        parts = [p for p in name.split("/") if p]
        for i, part in enumerate(parts):
            if part.endswith(LANCE_SUFFIX):
                dirs.add("/".join(parts[:i]))
                break
    return dirs


def _check_database(path: Path, label: str) -> str | None:
    """Open a database and ask SQLite whether it is sound."""
    try:
        conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    except sqlite3.Error as e:
        return f"{label} 打不开：{e}"
    try:
        row = conn.execute("PRAGMA integrity_check").fetchone()
    except sqlite3.Error as e:
        return f"{label} 查不出毛病：{e}"
    finally:
        conn.close()
    if not row:
        return f"{label} 没有查出结果"
    if row[0] != "ok":
        return f"{label} 损坏了：{row[0]}"
    return None


def _import_lancedb():
    """Find lancedb, if it is anywhere near.

    The script may be run by a plain system Python while lancedb lives
    in the backend's own environment. The vector store is the one part
    of a backup that cannot be checked by looking at the bytes, so it
    is worth reaching for the environment that can open it.
    """
    try:
        import lancedb
        return lancedb
    except ImportError:
        pass
    root = Path(__file__).resolve().parent.parent
    for site_packages in (root / "backend" / ".venv").glob("lib/python*/site-packages"):
        if not site_packages.is_dir():
            continue
        sys.path.insert(0, str(site_packages))
        try:
            import lancedb
            return lancedb
        except ImportError:
            sys.path.pop(0)
    return None


def _check_vector_store(path: Path, label: str) -> tuple[str | None, bool]:
    """Open a vector database and count its rows.

    Returns (problem, checked). A database that cannot be reached
    because lancedb is missing is not a failure -- it is a check that
    did not happen, and the caller says so.
    """
    lancedb = _import_lancedb()
    if lancedb is None:
        return None, False
    try:
        db = lancedb.connect(str(path))
        tables = db.list_tables()
        # LanceDB's list_tables hands back a wrapper in some versions
        # and a plain list in others. The names are what is wanted.
        names = list(getattr(tables, "tables", tables))
        for table in names:
            db.open_table(table).count_rows()
    except Exception as e:
        return f"{label} 打不开：{e}", True
    return None, True


class Report:
    """What the check found.

    Two kinds of thing can come back, and they are not the same. A
    problem means something is wrong: the archive must not be used. A
    note means something could not be looked at: the archive may be
    fine, but nobody has said so, and saying so is the point.
    """

    def __init__(self, problems: list[str] | None = None, notes: list[str] | None = None):
        self.problems = problems or []
        self.notes = notes or []

    @property
    def ok(self) -> bool:
        return not self.problems

    def __bool__(self) -> bool:
        return self.ok


def verify_archive(archive: Path) -> Report:
    """Look at the archive and report what is wrong with it.

    The problems are written for a person to read: which file, and
    what happened to it.
    """
    problems: list[str] = []
    notes: list[str] = []
    staging = Path(tempfile.mkdtemp(prefix="sfm-verify-"))
    try:
        try:
            tar = tarfile.open(archive, "r:gz")
        except (tarfile.TarError, OSError) as e:
            return Report(problems=[f"归档打不开：{e}"])
        with tar:
            members = tar.getmembers()
            names = [m.name for m in members]
            vector_dirs = _database_dirs(names)

            for member in members:
                if not member.isfile():
                    continue
                name = member.name
                if name.endswith(SQLITE_SIDE_SUFFIXES):
                    continue

                if name.endswith(".db"):
                    target = staging / ("db-" + name.replace("/", "_"))
                    try:
                        with tar.extractfile(member) as src, open(target, "wb") as dst:
                            shutil.copyfileobj(src, dst)
                    except (tarfile.TarError, OSError) as e:
                        problems.append(f"{name} 读不出来：{e}")
                        continue
                    problem = _check_database(target, name)
                    if problem:
                        problems.append(problem)
                    target.unlink(missing_ok=True)
                    continue

                if any(name.startswith(d + "/") for d in vector_dirs):
                    # Left for the vector store check below: a table is a
                    # directory, and it only opens as part of its own
                    # database.
                    continue

                # Everything else is the person's own files. They are not
                # inspected, but they are read, so that an archive cut
                # short in the middle of one is noticed here.
                try:
                    with tar.extractfile(member) as src:
                        while src.read(1024 * 1024):
                            pass
                except (tarfile.TarError, OSError) as e:
                    problems.append(f"{name} 读不出来：{e}")

            for vector_dir in sorted(vector_dirs):
                if not vector_dir:
                    continue
                target = staging / ("vec-" + vector_dir.replace("/", "_"))
                target.mkdir(parents=True, exist_ok=True)
                for member in members:
                    if not member.isfile() or not member.name.startswith(vector_dir + "/"):
                        continue
                    relative = member.name[len(vector_dir) + 1:]
                    out = target / relative
                    out.parent.mkdir(parents=True, exist_ok=True)
                    try:
                        with tar.extractfile(member) as src, open(out, "wb") as dst:
                            shutil.copyfileobj(src, dst)
                    except (tarfile.TarError, OSError) as e:
                        problems.append(f"{member.name} 读不出来：{e}")
                problem, checked = _check_vector_store(target, vector_dir)
                if problem:
                    problems.append(problem)
                elif not checked:
                    notes.append(f"{vector_dir} 没有检查（这个环境里没有 lancedb）")
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    return Report(problems=problems, notes=notes)

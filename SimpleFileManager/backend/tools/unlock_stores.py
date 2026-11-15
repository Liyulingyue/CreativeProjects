"""Move the stores off "one lock over everything".

The chat history had the same shape and it stopped the service talking:
one lock covered the reads as well as the writes, and the page that
lists the conversations held it for as long as the reading took. The
database is opened in WAL mode -- a reader works while a writer works
-- so the reads do not need the lock at all.

This does the same for the other stores: the reads let SQLite sort it
out, the writes take a lock with a deadline, and the slow things (a
password hash) happen outside it.

    python -m tools.unlock_stores
"""
import re
import sys
from pathlib import Path

BACKEND = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BACKEND))

# file -> (class, {read methods}, {write methods}, the name in a message)
STORES = [
    ("app/users.py", "UserStore", {
        "get_user", "get_by_username", "list_users", "count_admins",
        "list_restrictions", "token_username", "count_tokens",
    }, {
        "create_user", "set_password", "set_disabled", "delete_user",
        "restrict", "lift", "store_token", "revoke_token", "prune_tokens",
        "revoke_all_tokens",
    }, "用户表"),
    ("app/shares.py", "ShareStore", {
        "get", "get_password_hash", "list",
    }, {
        "create", "revoke", "set_expiry", "note_access", "purge_expired",
    }, "共享表"),
    ("app/upload_log.py", "UploadLog", {
        "list", "recent", "stats",
    }, {
        "record", "prune", "clear",
    }, "上传记录"),
    ("app/trash_retention.py", "TrashLog", {
        "list",
    }, {
        "record", "clear",
    }, "回收站记录"),
]


def class_range(lines, class_name):
    start = next((i for i, l in enumerate(lines) if l.startswith(f"class {class_name}")), None)
    if start is None:
        raise SystemExit(f"class {class_name} not found")
    end = next((i for i, l in enumerate(lines[start + 1:], start + 1)
                if l.startswith("class ")), len(lines))
    return start, end


def split_methods(lines, start, end):
    """(name, [lines]) for every method of the class.

    The lines before the first method -- the docstring, the attributes
    -- are kept as a chunk of their own, so nothing is lost.
    """
    chunks = []
    name = None
    buf = []
    head = []
    for line in lines[start:end]:
        m = re.match(r"    def (\w+)", line)
        if m:
            if name:
                chunks.append((name, buf))
            elif head:
                chunks.append((None, head))
            name, buf = m.group(1), [line]
        elif name:
            buf.append(line)
        else:
            head.append(line)
    if name:
        chunks.append((name, buf))
    elif head:
        chunks.append((None, head))
    return chunks


def main():
    for rel, cls, reads, writes, what in STORES:
        path = BACKEND / rel
        lines = path.read_text().split("\n")
        start, end = class_range(lines, cls)
        chunks = split_methods(lines, start, end)
        out_head = lines[:start]
        out_tail = lines[end:]
        body = []
        for name, chunk in chunks:
            text = "\n".join(chunk)
            if name is None:
                body.append(text)
                continue
            if name in reads and "with self._lock:" in text:
                # No lock for a read: SQLite sorts it out. Only the
                # lines that were inside the block move up; the ones
                # after it keep their place.
                new = []
                inside = False
                block_indent = 0
                for line in text.split("\n"):
                    if not inside:
                        if line.strip() == "with self._lock:":
                            inside = True
                            block_indent = len(line) - len(line.lstrip())
                            continue
                        new.append(line)
                        continue
                    # Inside the block: until a line comes back to the
                    # level of the "with" itself.
                    if line.strip() and (len(line) - len(line.lstrip())) <= block_indent:
                        inside = False
                        new.append(line)
                        continue
                    new.append(line[4:] if line.startswith(" " * (block_indent + 4)) else line)
                text = "\n".join(new)
                print(f"  {rel}: {cls}.{name} 不再拿锁")
            elif name in writes and "with self._lock:" in text:
                text = text.replace(
                    "with self._lock:",
                    f'with db.write_lock(self._lock, what="{what}"):')
                print(f"  {rel}: {cls}.{name} 用带上限的写锁")
            body.append(text)
        path.write_text("\n".join(out_head + body + out_tail))


if __name__ == "__main__":
    main()

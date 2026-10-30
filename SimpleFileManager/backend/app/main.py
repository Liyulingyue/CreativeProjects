from contextlib import asynccontextmanager
import os
import time
from fastapi import FastAPI, Depends, Request
from fastapi.security import HTTPAuthorizationCredentials
from fastapi.security import HTTPBearer
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from .logging_setup import setup_logging, start_request_id, request_id
from .routers import fs, search, settings, rag, chat, chat_history_router, agent, organizer, plans, digest, auth, get_current_auth

bearer_scheme = HTTPBearer(auto_error=False)
from .routers.auth import is_auth_enabled, set_password, verify_stream_token
from .deps import PathNotAllowed, resolve_under_root
from .indexer import indexer
from .auto_digest import auto_digest
from .routers import providers as providers_router  # noqa: E402
from .routers import embedding_providers as embedding_providers_router  # noqa: E402
from .routers.shares import shares as shares_router  # noqa: E402
from .routers.share_public import share_public as share_public_router  # noqa: E402
from .routers.users import users as users_router  # noqa: E402

# Before anything else, so the modules imported below log into the
# configured handlers rather than the root logger's default.
setup_logging()

import logging  # noqa: E402

log = logging.getLogger(__name__)


def _init_auth():
    """Initialize auth from an out-of-band secret, never from a public request."""
    init_pwd = os.getenv("INIT_PASSWORD", "").strip()
    if init_pwd and not is_auth_enabled():
        set_password(init_pwd)
    if not is_auth_enabled() and os.getenv("ALLOW_INSECURE_NO_AUTH", "").lower() != "true":
        raise RuntimeError(
            "Authentication is not configured. Set INIT_PASSWORD before startup. "
            "For isolated local development only, ALLOW_INSECURE_NO_AUTH=true disables this guard."
        )


@asynccontextmanager
async def lifespan(app: FastAPI):
    _init_auth()
    # The sessions from the old JSON file move into the database once.
    from .routers.auth import migrate_tokens

    migrate_tokens()
    log.info("starting: indexer and digest scheduler coming up")
    indexer.start()
    auto_digest.start()
    yield
    log.info("shutting down: stopping indexer and digest scheduler")
    indexer.stop()
    auto_digest.stop()


app = FastAPI(title="SimpleFileManager", version="0.4.0", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)


async def fs_auth(request: Request, credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
    """Authorise a file request.

    A media element cannot send an Authorization header, so a file can
    also be reached with a short-lived token in the URL -- one that is
    good for a single path and a few minutes. Without this the
    <video> and <audio> players would have to fetch the whole file
    into memory first, which defeats the point of streaming it.
    """
    stream_token = request.query_params.get("st")
    if stream_token:
        path = request.query_params.get("path", "")
        # The token is signed over the resolved path, so it is checked
        # against the resolved path too -- a relative path and the same
        # file named in full are one thing here.
        try:
            target = resolve_under_root(path)
        except Exception:
            target = None
        if target is not None and verify_stream_token(str(target), stream_token):
            return {"ok": True, "via": "stream-token"}
    return await get_current_auth(request, credentials)


@app.exception_handler(PathNotAllowed)
async def path_not_allowed_handler(request: Request, exc: PathNotAllowed):
    """Somebody asked for something they may not reach."""
    return JSONResponse(status_code=403, content={"detail": str(exc) or "没有访问这个位置的权限"})


@app.middleware("http")
async def request_context(request: Request, call_next):
    """Give every request an id and a line in the log.

    Without this a slow request leaves no trace: the access log says
    which URL was called but not how long it took, and nothing at all
    links a log line to the request that produced it.
    """
    rid = request.headers.get("x-request-id") or None
    start_request_id(rid)
    started = time.monotonic()
    try:
        response = await call_next(request)
    except Exception:
        elapsed = int((time.monotonic() - started) * 1000)
        log.exception(
            "%s %s -> unhandled after %dms", request.method, request.url.path, elapsed
        )
        raise
    elapsed = int((time.monotonic() - started) * 1000)
    response.headers["X-Request-ID"] = request_id()
    level = logging.WARNING if response.status_code >= 500 else logging.INFO
    client = request.client.host if request.client else "-"
    log.log(
        level,
        "%s %s %s -> %d in %dms",
        client,
        request.method,
        request.url.path,
        response.status_code,
        elapsed,
    )
    return response


# Auth router is public (no dependency)
app.include_router(auth, prefix="/api/auth", tags=["auth"])


# Health check is public
@app.get("/api/health")
def health():
    """Report whether the things this service depends on are actually up.

    A static "ok" is worse than no endpoint at all: it looks like a
    monitor is watching and reports green while the database is locked
    and the disk is full.
    """
    from .health import check_health
    report = check_health()
    status = 200 if report["status"] == "ok" else 503
    return JSONResponse(report, status_code=status)


# All other routers require auth
auth_dep = Depends(get_current_auth)
app.include_router(fs, prefix="/api/fs", tags=["fs"], dependencies=[Depends(fs_auth)])
app.include_router(search, prefix="/api/search", tags=["search"], dependencies=[auth_dep])
app.include_router(settings, prefix="/api/settings", tags=["settings"], dependencies=[auth_dep])
app.include_router(rag, prefix="/api/rag", tags=["rag"], dependencies=[auth_dep])
app.include_router(chat, prefix="/api/chat", tags=["chat"], dependencies=[auth_dep])
app.include_router(chat_history_router, prefix="/api/chat_history", tags=["chat_history"], dependencies=[auth_dep])
from .routers import providers as providers_router  # noqa: E402
app.include_router(providers_router.router, prefix="/api/providers", tags=["providers"], dependencies=[auth_dep])
app.include_router(embedding_providers_router.router, prefix="/api/embedding_providers", tags=["embedding_providers"], dependencies=[auth_dep])
app.include_router(agent, prefix="/api/agent", tags=["agent"], dependencies=[auth_dep])
app.include_router(organizer, prefix="/api/organizer", tags=["organizer"], dependencies=[auth_dep])
app.include_router(plans, tags=["plans"], dependencies=[auth_dep])
app.include_router(digest, tags=["digest"], dependencies=[auth_dep])
app.include_router(shares_router, prefix="/api/shares", tags=["shares"], dependencies=[auth_dep])

# The public side of a share. It has to be reachable without a login
# -- that is the point of a share -- so it is mounted without the
# auth dependency. Everything that could make it unsafe is handled by
# the rules on the share itself.
app.include_router(share_public_router, prefix="/api/share", tags=["share"])
app.include_router(users_router, prefix="/api/users", tags=["users"], dependencies=[auth_dep])

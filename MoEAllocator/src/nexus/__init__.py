__all__ = ["ExpertWorker", "NexusMaster"]


def __getattr__(name: str):
    if name == "ExpertWorker":
        from .worker import ExpertWorker

        return ExpertWorker
    if name == "NexusMaster":
        from .master import NexusMaster

        return NexusMaster
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")

from importlib.metadata import version, PackageNotFoundError
from .wrapper import OpenAIJsonWrapper

__all__ = [
    "OpenAIJsonWrapper",
]

try:
    __version__ = version("openaijsonwrapper")
except PackageNotFoundError:
    __version__ = "0.0.0"

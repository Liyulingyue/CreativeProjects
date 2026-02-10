from abc import ABC, abstractmethod
from typing import List, Dict, Any

class BasePlugin(ABC):
    """
    Base class for all FileStation plugins.
    """
    @property
    @abstractmethod
    def name(self) -> str:
        pass

    @abstractmethod
    async def process_file(self, file_path: str, metadata: Dict[str, Any]) -> None:
        """
        Logic for processing a file (e.g., extracting text).
        """
        pass

class PluginManager:
    """
    Simple registry for FileStation plugins.
    """
    def __init__(self):
        self._plugins: List[BasePlugin] = []

    def register(self, plugin: BasePlugin):
        self._plugins.append(plugin)
        print(f"Plugin registered: {plugin.name}")

    def get_plugins(self) -> List[BasePlugin]:
        return self._plugins

# Global instance for app-wide use
plugin_manager = PluginManager()

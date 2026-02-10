from abc import ABC, abstractmethod
from typing import List, Dict, Any, Optional

class StorageDriver(ABC):
    @abstractmethod
    def save_file(self, content: bytes, relative_path: str) -> str:
        """Saves file content and returns the physical path or identifier."""
        pass

    @abstractmethod
    def delete_item(self, relative_path: str) -> None:
        """Deletes a file or folder at the given relative path."""
        pass

    @abstractmethod
    def move_item(self, old_path: str, new_path: str) -> None:
        """Moves/renames a file or folder."""
        pass

    @abstractmethod
    def get_physical_path(self, relative_path: str, file_id: Optional[int] = None) -> str:
        """Returns the absolute path for file system access."""
        pass

    @abstractmethod
    def list_contents(self, prefix: str) -> Dict[str, Any]:
        """Lists folders and files under a prefix. Returns {'folders': [], 'files': []}."""
        pass

import numpy as np
from pathlib import Path
from typing import Optional, Literal
from PIL import Image
from .base import ImageItem, ImageGroup
from .deduplicator import Grouper


class EmbeddingGrouper(Grouper):
    def __init__(
        self,
        similarity_threshold: float = 0.85,
        model_name: str = "clip",
        batch_size: int = 32,
    ):
        self.similarity_threshold = similarity_threshold
        self.model_name = model_name
        self.batch_size = batch_size
        self._model = None
        self._preprocessor = None

    def _load_model(self):
        if self._model is not None:
            return

        if self.model_name == "clip":
            try:
                import torch
                from transformers import CLIPProcessor, CLIPModel
                self._model = CLIPModel.from_pretrained("openai/clip-vit-base-patch32")
                self._preprocessor = CLIPProcessor.from_pretrained("openai/clip-vit-base-patch32")
            except ImportError:
                raise ImportError("需要安装 transformers 和 torch: pip install transformers torch")

    def extract_feature(self, item: ImageItem) -> Optional[np.ndarray]:
        self._load_model()

        try:
            img = Image.open(item.path).convert("RGB")

            if self.model_name == "clip":
                inputs = self._preprocessor(images=img, return_tensors="pt")
                with torch.no_grad():
                    features = self._model.get_image_features(**inputs)
                item.embedding = features[0].numpy()
                return item.embedding
            else:
                raise ValueError(f"不支持的模型: {self.model_name}")
        except Exception:
            return None

    def compute_similarity(self, emb1: np.ndarray, emb2: np.ndarray) -> float:
        emb1 = emb1 / np.linalg.norm(emb1)
        emb2 = emb2 / np.linalg.norm(emb2)
        return float(np.dot(emb1, emb2))

    def should_group(self, feat1: np.ndarray, feat2: np.ndarray) -> bool:
        if feat1 is None or feat2 is None:
            return False
        return self.compute_similarity(feat1, feat2) >= self.similarity_threshold

    def _cluster_items(self, items: list[ImageItem], **kwargs) -> list[ImageGroup]:
        from scipy.cluster.hierarchy import linkage, fcluster
        from scipy.spatial.distance import pdist, squareform

        items_with_emb = [it for it in items if it.metadata.get("feature") is not None]

        if len(items_with_emb) < 2:
            return [ImageGroup(group_id=f"embedding_group_{i}", items=[it]) for it in items_with_emb]

        embeddings = np.array([it.metadata["feature"] for it in items_with_emb])
        embeddings = embeddings / np.linalg.norm(embeddings, axis=1, keepdims=True)

        distances = 1 - np.dot(embeddings, embeddings.T)
        distance_matrix = squareform(distances)

        linkage_matrix = linkage(pdist(embeddings), method="average")

        threshold = 1 - self.similarity_threshold
        cluster_labels = fcluster(linkage_matrix, t=threshold, criterion="distance")

        groups: dict[int, ImageGroup] = {}
        for item, label in zip(items_with_emb, cluster_labels):
            if label not in groups:
                groups[label] = ImageGroup(group_id=f"embedding_group_{len(groups)}")
            groups[label].add(item)

        return list(groups.values())


class ResNetGrouper(Grouper):
    def __init__(
        self,
        similarity_threshold: float = 0.85,
        model_name: str = "resnet50",
        batch_size: int = 32,
    ):
        self.similarity_threshold = similarity_threshold
        self.model_name = model_name
        self.batch_size = batch_size
        self._model = None

    def _load_model(self):
        if self._model is not None:
            return

        try:
            import torch
            import torchvision.models as models
            import torchvision.transforms as transforms

            if self.model_name == "resnet50":
                model = models.resnet50(weights=models.ResNet50_Weights.DEFAULT)
            elif self.model_name == "resnet18":
                model = models.resnet18(weights=models.ResNet18_Weights.DEFAULT)
            else:
                model = models.resnet50(weights=models.ResNet50_Weights.DEFAULT)

            self._model = torch.nn.Sequential(*list(model.children())[:-1])
            self._model.eval()
            self._transform = transforms.Compose([
                transforms.Resize(256),
                transforms.CenterCrop(224),
                transforms.ToTensor(),
                transforms.Normalize(mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225]),
            ])
        except ImportError:
            raise ImportError("需要安装 torch 和 torchvision: pip install torch torchvision")

    def extract_feature(self, item: ImageItem) -> Optional[np.ndarray]:
        self._load_model()

        try:
            import torch

            img = Image.open(item.path).convert("RGB")
            tensor = self._transform(img).unsqueeze(0)

            with torch.no_grad():
                features = self._model(tensor)
                item.embedding = features.squeeze().numpy()

            return item.embedding
        except Exception:
            return None

    def compute_similarity(self, emb1: np.ndarray, emb2: np.ndarray) -> float:
        emb1 = emb1 / np.linalg.norm(emb1)
        emb2 = emb2 / np.linalg.norm(emb2)
        return float(np.dot(emb1, emb2))

    def should_group(self, feat1: np.ndarray, feat2: np.ndarray) -> bool:
        if feat1 is None or feat2 is None:
            return False
        return self.compute_similarity(feat1, feat2) >= self.similarity_threshold

    def _cluster_items(self, items: list[ImageItem], **kwargs) -> list[ImageGroup]:
        from scipy.cluster.hierarchy import linkage, fcluster
        from scipy.spatial.distance import pdist, squareform

        items_with_emb = [it for it in items if it.metadata.get("feature") is not None]

        if len(items_with_emb) < 2:
            return [ImageGroup(group_id=f"resnet_group_{i}", items=[it]) for it in items_with_emb]

        embeddings = np.array([it.metadata["feature"] for it in items_with_emb])
        embeddings = embeddings / np.linalg.norm(embeddings, axis=1, keepdims=True)

        distances = 1 - np.dot(embeddings, embeddings.T)
        distance_matrix = squareform(distances)

        linkage_matrix = linkage(pdist(embeddings), method="average")

        threshold = 1 - self.similarity_threshold
        cluster_labels = fcluster(linkage_matrix, t=threshold, criterion="distance")

        groups: dict[int, ImageGroup] = {}
        for item, label in zip(items_with_emb, cluster_labels):
            if label not in groups:
                groups[label] = ImageGroup(group_id=f"resnet_group_{len(groups)}")
            groups[label].add(item)

        return list(groups.values())


def create_embedding_grouper(
    model: Literal["clip", "resnet50", "resnet18"] = "clip",
    **kwargs
) -> Grouper:
    if model == "clip":
        return EmbeddingGrouper(**kwargs)
    elif model in ("resnet50", "resnet18"):
        return ResNetGrouper(model=model, **kwargs)
    else:
        raise ValueError(f"不支持的模型: {model}")

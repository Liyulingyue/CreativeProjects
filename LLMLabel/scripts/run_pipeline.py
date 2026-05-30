import os
import argparse
from pathlib import Path
from dotenv import load_dotenv

from src.task_type import TaskType
from src.text_labeler import TextLLMLabeler
from src.text_trainer import TextTrainer, TextTrainConfig
from src.text_classifier import TextClassifier, TextClassifyConfig
from src.image_labeler import ImageLLMLabeler
from src.image_trainer import ImageTrainer, ImageTrainConfig
from src.image_classifier import ImageClassifier, ImageClassifyConfig
from src.utils import sample_texts, load_all_texts


def stage1_label(config: dict, task_type: TaskType):
    print("\n" + "=" * 50)
    print(f"阶段一：大模型标注 [{task_type.value}]")
    print("=" * 50)

    if task_type == TaskType.TEXT_CLASSIFICATION:
        labeler = TextLLMLabeler(
            api_key=config["api_key"],
            base_url=config.get("base_url", "https://api.openai.com/v1"),
            model_name=config.get("model_name", "gpt-4o"),
            categories=config.get("categories", ["正例", "负例"]),
        )
        texts = sample_texts(config["raw_data_path"], config["sample_size"])
        print(f"[阶段一] 采样 {len(texts)} 条文本")
        items = labeler.label_batch(texts, instruction=config.get("instruction", ""), show_progress=True)

    elif task_type == TaskType.IMAGE_CLASSIFICATION:
        labeler = ImageLLMLabeler(
            api_key=config["api_key"],
            base_url=config.get("base_url", "https://api.openai.com/v1"),
            model_name=config.get("model_name", "gpt-4o"),
            categories=config.get("categories", ["正例", "负例"]),
        )
        from src.utils import load_image_paths
        image_paths = load_image_paths(config["raw_data_path"], config["sample_size"])
        print(f"[阶段一] 采样 {len(image_paths)} 张图片")
        items = labeler.label_batch(image_paths, instruction=config.get("instruction", ""), show_progress=True)

    output_path = config.get("labeled_output", "LLMLabel/data/labeled/labeled.jsonl")
    labeler.save(items, output_path)
    print(f"[阶段一] 标注完成，保存至：{output_path}")


def stage2_train(config: dict, task_type: TaskType):
    print("\n" + "=" * 50)
    print(f"阶段二：模型训练 [{task_type.value}]")
    print("=" * 50)

    labeled_path = config.get("labeled_output", "LLMLabel/data/labeled/labeled.jsonl")

    if task_type == TaskType.TEXT_CLASSIFICATION:
        train_config = TextTrainConfig(
            model_name=config.get("bert_model", "hfl/chinese-roberta-wwm-ext"),
            output_dir=config.get("model_output_dir", "LLMLabel/data/output/model"),
            num_epochs=config.get("num_epochs", 3),
            batch_size=config.get("batch_size", 16),
            learning_rate=config.get("learning_rate", 2e-5),
        )
        trainer = TextTrainer(train_config)

    elif task_type == TaskType.IMAGE_CLASSIFICATION:
        train_config = ImageTrainConfig(
            model_name=config.get("image_model", "google/vit-base-patch16-224"),
            output_dir=config.get("model_output_dir", "LLMLabel/data/output/model"),
            num_epochs=config.get("num_epochs", 3),
            batch_size=config.get("batch_size", 16),
            learning_rate=config.get("learning_rate", 2e-5),
        )
        trainer = ImageTrainer(train_config)

    trainer.train(labeled_path)


def stage3_classify(config: dict, task_type: TaskType):
    print("\n" + "=" * 50)
    print(f"阶段三：小模型分类 [{task_type.value}]")
    print("=" * 50)

    model_path = config.get("model_output_dir", "LLMLabel/data/output/model/final")
    raw_data_path = config.get("raw_data_path")
    output_path = config.get("final_output", "LLMLabel/data/output/classified.jsonl")

    if task_type == TaskType.TEXT_CLASSIFICATION:
        texts = load_all_texts(raw_data_path)
        print(f"[阶段三] 加载 {len(texts)} 条文本")
        clf = TextClassifier(TextClassifyConfig(
            model_path=model_path,
            device=config.get("device", "cuda"),
        ))
        items = clf.classify_batch(texts, instruction=config.get("instruction", ""), show_progress=True)

    elif task_type == TaskType.IMAGE_CLASSIFICATION:
        from src.utils import load_image_paths
        image_paths = load_image_paths(raw_data_path)
        print(f"[阶段三] 加载 {len(image_paths)} 张图片")
        clf = ImageClassifier(ImageClassifyConfig(
            model_path=model_path,
            device=config.get("device", "cuda"),
        ))
        items = clf.classify_batch(image_paths, instruction=config.get("instruction", ""), show_progress=True)

    clf.save(items, output_path)
    print(f"[阶段三] 分类完成，保存至：{output_path}")


def main():
    parser = argparse.ArgumentParser(description="LLMLabel 多任务分类流水线")
    parser.add_argument("--stage", type=int, choices=[1, 2, 3], required=True,
                        help="阶段：1=大模型标注, 2=模型训练, 3=模型分类")
    parser.add_argument("--task", type=str, default="text",
                        choices=["text", "image"],
                        help="任务类型：text=文本分类, image=图片分类")
    parser.add_argument("--raw-data", type=str, default="LLMLabel/data/raw/texts.jsonl",
                        help="原始数据路径（文本为 .jsonl，图片为目录或 .jsonl）")
    parser.add_argument("--labeled-output", type=str,
                        default="LLMLabel/data/labeled/labeled.jsonl",
                        help="标注结果保存路径")
    parser.add_argument("--model-output", type=str,
                        default="LLMLabel/data/output/model/final",
                        help="模型保存路径")
    parser.add_argument("--final-output", type=str,
                        default="LLMLabel/data/output/classified.jsonl",
                        help="最终分类结果路径")
    parser.add_argument("--sample-size", type=int, default=10000,
                        help="大模型标注的样本数量")
    parser.add_argument("--num-epochs", type=int, default=3,
                        help="训练轮数")
    parser.add_argument("--batch-size", type=int, default=16,
                        help="训练批次大小")
    parser.add_argument("--learning-rate", type=float, default=2e-5)
    parser.add_argument("--bert-model", type=str,
                        default="hfl/chinese-roberta-wwm-ext",
                        help="文本分类模型")
    parser.add_argument("--image-model", type=str,
                        default="google/vit-base-patch16-224",
                        help="图片分类模型")
    parser.add_argument("--device", type=str, default="cuda",
                        help="推理设备：cuda 或 cpu")
    parser.add_argument("--categories", type=str, default="正例,负例",
                        help="分类类别，逗号分隔")
    parser.add_argument("--instruction", type=str, default="",
                        help="分类指令")

    args = parser.parse_args()
    load_dotenv()

    task_type = TaskType.from_string(args.task)

    config = {
        "api_key": os.getenv("MODEL_KEY", ""),
        "base_url": os.getenv("MODEL_URL", "https://api.openai.com/v1"),
        "model_name": os.getenv("MODEL_NAME", "gpt-4o"),
        "raw_data_path": args.raw_data,
        "labeled_output": args.labeled_output,
        "model_output_dir": args.model_output,
        "final_output": args.final_output,
        "sample_size": args.sample_size,
        "num_epochs": args.num_epochs,
        "batch_size": args.batch_size,
        "learning_rate": args.learning_rate,
        "bert_model": args.bert_model,
        "image_model": args.image_model,
        "device": args.device,
        "categories": args.categories.split(","),
        "instruction": args.instruction,
    }

    if args.stage == 1:
        stage1_label(config, task_type)
    elif args.stage == 2:
        stage2_train(config, task_type)
    elif args.stage == 3:
        stage3_classify(config, task_type)


if __name__ == "__main__":
    main()

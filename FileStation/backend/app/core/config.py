import os

class Settings:
    """
    FileStation 统一配置类
    支持从环境变量读取，默认为本地开发配置
    """
    def __init__(self):
        # 数据库配置
        self.DATABASE = os.getenv("DATABASE", "filestation.db")
        self.USE_DATABASE = os.getenv("USE_DATABASE", "true").lower() == "true"
        
        # 存储配置
        self.UPLOAD_DIR = os.getenv("UPLOAD_DIR", "uploads")
        # 模式可选: "path" (直接路径) 或 "cas" (内容寻址)
        self.STORAGE_MODE = os.getenv("STORAGE_MODE", "path")
        
        # 应用信息
        self.APP_NAME = "FileStation"
        self.VERSION = "0.3.0"
        
        # 初始化物理目录
        os.makedirs(self.UPLOAD_DIR, exist_ok=True)

settings = Settings()

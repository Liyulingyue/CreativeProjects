#!/bin/bash

# 清理项目中的 __pycache__ 缓存目录

echo "正在清理项目中的 __pycache__ 缓存目录..."

# 查找并删除所有 __pycache__ 目录
find . -type d -name "__pycache__" -exec rm -rf {} +

echo "清理完成！"
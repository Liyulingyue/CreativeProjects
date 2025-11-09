#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
使用OpenCV从摄像头抓取图像并保存到文件
"""

import cv2
import sys

def capture_image(filename='captured_image.jpg', camera_index=0):
    """
    从摄像头抓取一张图像并保存到文件

    Args:
        filename: 保存的文件名 (默认: captured_image.jpg)
        camera_index: 摄像头索引 (默认: 0)
    """
    # 打开摄像头
    cap = cv2.VideoCapture(camera_index)
    
    if not cap.isOpened():
        print(f"无法打开摄像头 {camera_index}")
        return False
    
    print("摄像头已打开")
    
    ret, frame = cap.read()
    if not ret:
        print("无法读取帧")
    else:
        cv2.imwrite(filename, frame)
    
    # 释放资源
    cap.release()
    return True

if __name__ == "__main__":
    # 从命令行参数获取文件名
    filename = sys.argv[1] if len(sys.argv) > 1 else 'captured_image.jpg'
    camera_index = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    
    capture_image(filename, camera_index)
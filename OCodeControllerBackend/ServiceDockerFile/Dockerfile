FROM ubuntu:22.04

# 1. 安装基础依赖
RUN apt-get update && apt-get install -y \
    curl \
    supervisor \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# 2. 安装 OpenCode 服务
RUN curl -fsSL https://opencode.ai/install | bash

# 3. 安装 File Browser 服务
RUN curl -fsSL https://github.com/filebrowser/filebrowser/releases/download/v2.27.0/linux-amd64-filebrowser.tar.gz | tar -xz -C /usr/local/bin/

# 4. 配置环境变量与工作目录
ENV PATH="/root/.opencode/bin:$PATH"
RUN mkdir -p /var/lib/filebrowser
WORKDIR /

# 5. 暴露端口：4096 (OpenCode) 和 8080 (File Browser)
EXPOSE 4096 8080

# 6. OpenCode 环境变量（运行时通过 -e 注入）
ENV OPENCODE_SERVER_USERNAME=opencode
ENV OPENCODE_SERVER_PASSWORD=

# 7. FileBrowser 环境变量（运行时通过 -e 注入）
ENV FB_USERNAME=admin
ENV FB_PASSWORD=admin123

# 8. 复制 supervisor 配置
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf

# 9. 复制启动脚本（运行时初始化 FileBrowser 数据库）
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# 10. 启动时先初始化 FileBrowser，再启动 supervisor
ENTRYPOINT ["/entrypoint.sh"]

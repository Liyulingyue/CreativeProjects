FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    curl \
    && curl -fsSL https://opencode.ai/install | bash \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update && apt-get install -y \
    curl \
    && curl -fsSL https://opencode.ai/install | bash \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

ENV PATH="/root/.opencode/bin:$PATH"

EXPOSE 4096

ENV OPENCODE_SERVER_USERNAME=opencode
ENV OPENCODE_SERVER_PASSWORD=

CMD ["opencode", "web", "--hostname", "0.0.0.0", "--port", "4096"]

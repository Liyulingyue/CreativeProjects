FROM python:3.12-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc g++ \
    && rm -rf /var/lib/apt/lists/*

# The pinned versions, not the ranges. A rebuild six months from now
# with loose requirements would pick up whatever the packages have
# become since -- lancedb alone has gone from 0.15 to 0.40.
COPY requirements.lock .
RUN pip install --no-cache-dir -r requirements.lock

COPY . .

ENV STORAGE_PATH=/data
EXPOSE 8000

CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]

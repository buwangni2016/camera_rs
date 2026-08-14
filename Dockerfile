# ============================================================
# 构建阶段
# ============================================================
FROM rust:1.80-slim AS builder

WORKDIR /app

# 安装系统依赖（Linux V4L2 + OpenSSL）
RUN apt-get update && apt-get install -y \
    libv4l-dev v4l-utils pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# ============================================================
# 运行阶段
# ============================================================
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    libv4l-0 v4l-utils ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/camera_rs .

# 创建存储目录
RUN mkdir -p captures/{photos,videos,motion,auto,alerts,timelapse} faces

EXPOSE 5000

# 挂载摄像头设备（运行时通过 --device 指定）
VOLUME ["/app/captures", "/app/config.toml"]

CMD ["./camera_rs"]

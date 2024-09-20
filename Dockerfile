FROM docker.io/rustembedded/cross:aarch64-unknown-linux-gnu

# Enable multiarch to install arm64 libraries
RUN dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install -y \
    libudev-dev:arm64 \
    pkg-config:arm64 && \
    rm -rf /var/lib/apt/lists/*


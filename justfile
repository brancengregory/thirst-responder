default:
  cross build --target=aarch64-unknown-linux-gnu
release:
  cross build --target=aarch64-unknown-linux-gnu --release

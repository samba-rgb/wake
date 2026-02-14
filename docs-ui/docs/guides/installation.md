---
sidebar_position: 1
---

# Installation

You can install Wake using one of the following methods.

## macOS (Homebrew)

```bash
brew install samba-rgb/wake/wake
```

## Building from Source

### Prerequisites

- Rust toolchain (1.70.0 or later)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- Git

### Building

1. Clone the repository:
   ```bash
   git clone https://github.com/samba-rgb/wake.git
   cd wake
   ```

2. Build the project:
   ```bash
   # Development build
   cargo build

   # Release build with optimizations
   cargo build --release
   ```

   The binary will be available at:
   - Development build: `target/debug/wake`
   - Release build: `target/release/wake`

### Installation

After building, you can install the binary to your system:

```bash
# Install to ~/.cargo/bin
cargo install --path .

# Or copy the release binary to a location in your PATH
cp target/release/wake ~/.local/bin/
```

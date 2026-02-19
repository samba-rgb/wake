---
sidebar_position: 1
---

# Installation

Wake is currently built and tested for **macOS**, with **Linux support actively in development**. Choose the installation method that works best for your platform.

## Platform Support Status

- **✅ macOS**: Fully supported and tested
- **🚧 Linux**: Currently in development 
- **📋 Windows**: Planned for future releases

## macOS Installation (Recommended)

### Option 1: Homebrew (Easiest)

```bash
brew install samba-rgb/wake/wake
```

### Option 2: Building from Source

#### Prerequisites

- Rust toolchain (1.70.0 or later)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- Git

#### Building

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

#### Installation

After building, you can install the binary to your system:

```bash
# Install to ~/.cargo/bin
cargo install --path .

# Or copy the release binary to a location in your PATH
cp target/release/wake ~/.local/bin/
```

## Recommended Terminal Setup

For the best Wake experience with enhanced color visualization and terminal features:

### macOS: iTerm2 (Highly Recommended)

Wake's color-coded output, interactive UI, and visual elements work best with **iTerm2**:

```bash
# Install iTerm2 via Homebrew
brew install --cask iterm2
```

**Why iTerm2?**
- Enhanced 256-color and truecolor support
- Better Unicode rendering for Wake's UI elements  
- Split panes for monitoring multiple deployments
- Advanced search and highlighting features

### Linux: Modern Terminal Emulators

For Linux users (when support becomes available), use terminals with good color support:
- **GNOME Terminal** (default on Ubuntu)
- **Konsole** (KDE)
- **Alacritty** (cross-platform, GPU-accelerated)
- **Kitty** (fast, feature-rich)

### Verify Color Support

Test your terminal's color capabilities:

```bash
# After installing Wake, test colors
wake --version

# Check if your terminal supports 256 colors
echo $TERM
```

## Linux Installation (Development)

Linux support is currently in development. You can build from source, but some features may not work optimally yet.

### Building on Linux

// ...existing code...

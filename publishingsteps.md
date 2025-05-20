# Publishing Wake to the Internet

This document outlines the steps required to publish the Wake project so that anyone can use it.

## 1. Publish to GitHub

First, make sure your code is hosted in a public GitHub repository:

```bash
# Add all files to git (including any untracked files like icons)
git add .

# Commit your changes with a descriptive message
git commit -m "Prepare Wake for initial release"

# Create a new repository on GitHub at https://github.com/new
# Then add the remote and push your code
git remote add origin https://github.com/yourusername/wake.git
git push -u origin master
```

## 2. Publish to crates.io (Rust Package Registry)

Publishing to crates.io will allow users to install Wake directly with Cargo:

```bash
# Create an account on crates.io if you don't have one
# Visit https://crates.io/ and sign up

# Get an API token from https://crates.io/me

# Login to crates.io
cargo login <your-api-token>

# Make sure your Cargo.toml has the necessary metadata
# - Update description, license, repository URL
# - Add keywords and categories

# Check that your package is ready for publishing
cargo publish --dry-run

# Publish to crates.io
cargo publish
```

After publishing, users can install Wake with:
```bash
cargo install wake
```

## 3. Create Binary Releases for Non-Rust Users

Set up automated builds for multiple platforms using GitHub Actions:

1. Create a `.github/workflows/release.yml` file:

```yaml
name: Build and Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        include:
          - os: ubuntu-latest
            artifact_name: wake
            asset_name: wake-linux-amd64
          - os: macos-latest
            artifact_name: wake
            asset_name: wake-macos-amd64
          - os: windows-latest
            artifact_name: wake.exe
            asset_name: wake-windows-amd64.exe

    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true
      - uses: actions-rs/cargo@v1
        with:
          command: build
          args: --release
      - name: Upload binaries to release
        uses: svenstaro/upload-release-action@v2
        with:
          repo_token: ${{ secrets.GITHUB_TOKEN }}
          file: target/release/${{ matrix.artifact_name }}
          asset_name: ${{ matrix.asset_name }}
          tag: ${{ github.ref }}
```

2. Create and push a release tag:

```bash
# Create a tag for the release
git tag -a v0.1.0 -m "Initial release"

# Push the tag to GitHub
git push origin v0.1.0
```

## 4. Create a Homebrew Formula (for macOS users)

To make installation easier for macOS users:

1. Fork the homebrew-core repository or create your own tap
2. Create a formula file named `wake.rb`:

```ruby
class Wake < Formula
  desc "Command-line tool for tailing multiple pods and containers in Kubernetes clusters"
  homepage "https://github.com/yourusername/wake"
  url "https://github.com/yourusername/wake/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "<sha256-of-your-release-tarball>"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--root", prefix, "--path", "."
  end

  test do
    assert_match "wake", shell_output("#{bin}/wake --version")
  end
end
```

3. To get the sha256 of your release tarball:

```bash
wget https://github.com/yourusername/wake/archive/refs/tags/v0.1.0.tar.gz
shasum -a 256 v0.1.0.tar.gz
```

## 5. Create a Docker Image

For users who prefer containerized applications:

1. Create a `Dockerfile` in your project root:

```dockerfile
FROM rust:1.70-slim as builder
WORKDIR /usr/src/wake
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/wake/target/release/wake /usr/local/bin/wake
ENTRYPOINT ["wake"]
```

2. Build and publish to Docker Hub:

```bash
# Login to Docker Hub
docker login

# Build your image
docker build -t yourusername/wake:latest .

# Tag with version
docker tag yourusername/wake:latest yourusername/wake:0.1.0

# Push to Docker Hub
docker push yourusername/wake:latest
docker push yourusername/wake:0.1.0
```

## 6. Create Installation Instructions

Update your README.md to include clear installation instructions:

```markdown
## Installation

### Option 1: Using Cargo (Rust Package Manager)

If you have Rust installed, you can install Wake directly from crates.io:

```bash
cargo install wake
```

### Option 2: Binary Download

Download pre-compiled binaries for your platform:

- [Linux](https://github.com/yourusername/wake/releases/latest/download/wake-linux-amd64)
- [macOS](https://github.com/yourusername/wake/releases/latest/download/wake-macos-amd64)
- [Windows](https://github.com/yourusername/wake/releases/latest/download/wake-windows-amd64.exe)

Make the binary executable (Linux/macOS):
```bash
chmod +x wake-*-amd64
./wake-*-amd64
```

### Option 3: Using Homebrew (macOS users)

```bash
brew install yourusername/tap/wake
```

### Option 4: Using Docker

```bash
docker run --rm -it -v ~/.kube:/root/.kube yourusername/wake:latest
```
```

## 7. Setup GitHub Pages (Optional)

Create a simple project website using GitHub Pages:

1. Create a `gh-pages` branch or use the `docs/` folder in your main branch
2. Create an `index.html` file with project information and getting started guide
3. Enable GitHub Pages in your repository settings

## 8. Promote Your Project

Make sure people know about your project:

1. Share on relevant communities:
   - Reddit: r/rust, r/kubernetes, r/devops
   - HackerNews (if appropriate)
   - Twitter/X with relevant hashtags (#rust #kubernetes #devops)

2. Create content to showcase the tool:
   - Write a blog post explaining the project
   - Create a short demo video showing the tool in action
   - Make a screenshot or GIF showing the features

3. Submit to newsletters and aggregators:
   - This Week in Rust
   - Awesome-rust list
   - CNCF project landscape (if applicable)

## 9. Maintaining and Growing

Keep your project healthy:

1. Set up automated testing and CI/CD workflows
2. Respond to issues and pull requests promptly
3. Release updates regularly
4. Gather feedback and iterate on the design
5. Build a community around the project by being welcoming to contributors

Remember to update version numbers in all relevant places when releasing new versions.
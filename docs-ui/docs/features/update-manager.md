---
sidebar_position: 9
---

# Update Manager

Keep Wake up to date with the latest features and improvements.

## Versioning

Wake follows semantic versioning: **MAJOR.MINOR.PATCH**.

- **MAJOR** — incompatible API or behaviour changes (breaking changes).
- **MINOR** — new features, backwards-compatible.
- **PATCH** — bug fixes and small improvements, backwards-compatible.

Examples: `1.2.0` → new features since `1.1.x`; `2.0.0` → breaking changes from `1.x`.

Check the Wake version locally:

```bash
wake --version
# or
wake -V
```

## Update via Homebrew (macOS)

Update the Homebrew package on macOS:

```bash
brew upgrade samba-rgb/wake/wake
```

## Update from Source

If you installed from source, update and re-run:

```bash
cd wake
git pull origin main   # or the branch you track
cargo run              # run latest changes

# Or rebuild release
cargo build --release
./target/release/wake --help
```

## Release Notes

Check the [GitHub Releases page](https://github.com/samba-rgb/wake/releases) for detailed release notes and changelogs for each version.

## Automated Updates

Wake can check for updates automatically:

```bash
# Check for available updates
wake --check-updates

# Enable automatic update notifications
wake setconfig auto_update_check true
```

## Troubleshooting Updates

If you encounter issues after updating:

1. **Clear configuration cache**: `wake setconfig --reset`
2. **Rebuild from source**: Follow the source installation steps
3. **Check compatibility**: Ensure your Kubernetes version is supported
4. **Verify installation**: Run `wake --version` to confirm the update
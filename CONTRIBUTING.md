# Contributing to Wake

Thank you for your interest in contributing to Wake! This document provides guidelines and information for contributors.

## 🚀 About Wake

Wake is a powerful command-line tool for tailing multiple pods and containers in Kubernetes clusters. It features an interactive UI mode, advanced pattern filtering, template system for diagnostic tasks, and much more.

## 📋 Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Code Style & Standards](#code-style--standards)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)
- [Project Structure](#project-structure)
- [Release Process](#release-process)

## 🏁 Getting Started

### Prerequisites

- **Rust**: Version 1.70.0 or later
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Git**: For version control
- **Kubernetes cluster**: For testing (minikube, kind, or real cluster)
- **kubectl**: Kubernetes command-line tool

### First-time Setup

1. **Fork and clone the repository:**
   ```bash
   git clone https://github.com/YOUR_USERNAME/wake.git
   cd wake
   ```

2. **Set up the development environment:**
   ```bash
   # Install development dependencies
   cargo install cargo-watch cargo-audit cargo-husky
   
   # Set up git hooks
   cargo husky install
   ```

3. **Build and test:**
   ```bash
   # Development build
   cargo build
   
   # Run tests
   cargo test
   
   # Run the binary
   ./target/debug/wake --help
   ```

## 🛠️ Development Setup

### Development Workflow

```bash
# Auto-rebuild on changes
cargo watch -x build

# Run tests in watch mode
cargo watch -x test

# Check code formatting
cargo fmt --all -- --check

# Run linter
cargo clippy -- -D warnings

# Security audit
cargo audit
```

### Local Testing Environment

For setting up a local Kubernetes test environment with sample applications, see the [Development Environment Guide](dev/README.md).

```bash
# Set up test environment
cd dev
./scripts/setup.sh

# Run performance tests
cd perf
./setup-perf.sh
./benchmark.sh
```

## 🤝 How to Contribute

### Types of Contributions

We welcome various types of contributions:

- **🐛 Bug Reports**: Found a bug? Please report it!
- **✨ Feature Requests**: Have an idea? We'd love to hear it!
- **📚 Documentation**: Help improve our docs
- **🧪 Testing**: Add or improve tests
- **💡 Code**: Fix bugs or implement new features
- **🎨 UI/UX**: Improve the interactive UI experience

### Getting Started with Contributions

1. **Check existing issues**: Look for existing issues or create a new one
2. **Discuss first**: For major changes, discuss your approach first
3. **Create a branch**: Use descriptive branch names
4. **Make changes**: Follow our code style guidelines
5. **Test thoroughly**: Ensure your changes work as expected
6. **Submit a PR**: Create a pull request with a clear description

## 📝 Code Style & Standards

### Rust Code Style

- Follow standard Rust formatting with `cargo fmt`
- Use `cargo clippy` and address all warnings
- Write idiomatic Rust code
- Use meaningful variable and function names
- Add documentation comments for public APIs

### Code Organization

```
src/
├── lib.rs              # Library entry point
├── main.rs             # Binary entry point
├── cli/                # CLI argument parsing
├── core/               # Core functionality
├── ui/                 # Terminal UI components
├── templates/          # Template system
├── k8s/                # Kubernetes integration
├── filtering/          # Log filtering logic
├── output/             # Output formatting
└── ...
```

### Documentation Standards

- Document all public functions and structs
- Use clear, concise descriptions
- Include examples in doc comments when helpful
- Update README.md for user-facing changes

### Commit Message Format

Use conventional commit format:

```
feat: add support for custom output templates
fix: resolve memory leak in log buffering
docs: update installation instructions
test: add unit tests for filtering logic
refactor: simplify kubernetes client initialization
```

## 🧪 Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test core::tests

# Run integration tests
cargo test --test integration

# Run with output
cargo test -- --nocapture
```

### Test Categories

1. **Unit Tests**: Test individual functions and modules
2. **Integration Tests**: Test component interactions
3. **End-to-End Tests**: Test complete workflows

### Writing Tests

- Write tests for all new functionality
- Include edge cases and error conditions
- Use descriptive test names
- Mock external dependencies when appropriate

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_filtering_with_include_pattern() {
        // Test implementation
    }
}
```

## 📬 Pull Request Process

### Before Submitting

1. **Update documentation** if needed
2. **Add or update tests** for your changes
3. **Run the full test suite**: `cargo test`
4. **Check formatting**: `cargo fmt`
5. **Run clippy**: `cargo clippy`
6. **Update CHANGELOG.md** if applicable

### PR Template

When creating a pull request, include:

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Other (specify)

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing completed

## Checklist
- [ ] Code follows style guidelines
- [ ] Tests pass locally
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
```

### Review Process

1. **Automated checks**: CI/CD must pass
2. **Code review**: At least one maintainer review
3. **Testing**: Verify functionality works as expected
4. **Merge**: Squash and merge when approved

## 🐛 Issue Guidelines

### Bug Reports

When reporting bugs, include:

```markdown
**Environment:**
- Wake version: 
- OS: 
- Kubernetes version: 
- kubectl version: 

**Expected behavior:**
What you expected to happen

**Actual behavior:**
What actually happened

**Steps to reproduce:**
1. 
2. 
3. 

**Additional context:**
Logs, screenshots, etc.
```

### Feature Requests

For feature requests, include:

```markdown
**Problem Statement:**
What problem does this solve?

**Proposed Solution:**
Describe your proposed solution

**Alternatives:**
Other solutions you considered

**Additional Context:**
Screenshots, examples, etc.
```

## 📁 Project Structure

### Key Directories

- **`src/`**: Main source code
- **`tests/`**: Integration tests
- **`docs/`**: Documentation files
- **`dev/`**: Development environment setup
- **`docs-ui/`**: Docusaurus documentation site
- **`resources/`**: Static resources and assets

### Important Files

- **`Cargo.toml`**: Dependencies and project metadata
- **`README.md`**: Main project documentation
- **`CHANGELOG.md`**: Version history
- **`LICENSE`**: MIT license
- **`.github/workflows/`**: CI/CD configuration

## 🚢 Release Process

### Version Management

We follow semantic versioning (SemVer):
- **Major** (1.0.0): Breaking changes
- **Minor** (0.1.0): New features (backward compatible)
- **Patch** (0.0.1): Bug fixes

### Release Checklist

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Update documentation if needed
4. Run full test suite
5. Create release PR
6. Tag release after merge
7. Publish to crates.io (maintainers)
8. Update Homebrew formula (maintainers)

## 🎯 Areas for Contribution

### High Priority

- **Testing**: Add more unit and integration tests
- **Documentation**: Improve examples and guides
- **Performance**: Optimize log processing and filtering
- **Templates**: Add more diagnostic templates

### Medium Priority

- **Features**: Custom output templates, label filtering
- **UI/UX**: Improve interactive UI experience
- **Configuration**: Add configuration file support
- **Shell completion**: Add bash/zsh/fish completion

### Good First Issues

Look for issues labeled `good-first-issue` or `help-wanted` on GitHub.

## 💬 Community & Support

- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: General questions and ideas
- **Documentation**: Check the [Wake website](https://www.wakelog.in)

## 📄 License

By contributing to Wake, you agree that your contributions will be licensed under the MIT License.

## 🙏 Recognition

Contributors are recognized in:
- Release notes
- CHANGELOG.md
- GitHub contributors page

Thank you for contributing to Wake! 🚀
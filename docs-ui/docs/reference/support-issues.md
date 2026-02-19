---
sidebar_position: 2
---

# Support & Issues

If you encounter any issues, please raise them on the [GitHub Issues page](https://github.com/samba-rgb/wake/issues).

For direct contact, you can reach the author at [samba24052001@gmail.com](mailto:samba24052001@gmail.com).

## How to Report Issues

When reporting issues, please include:

1. **Wake version**: Run `wake --version`
2. **Operating system**: Linux, macOS, Windows
3. **Kubernetes version**: Run `kubectl version --short`
4. **Error message**: Full error output if available
5. **Steps to reproduce**: What command caused the issue
6. **Expected behavior**: What you expected to happen
7. **Actual behavior**: What actually happened

## Common Issues

### Connection Problems
- Verify your kubeconfig is correct: `kubectl config current-context`
- Check cluster connectivity: `kubectl get pods`
- Ensure proper RBAC permissions for log access

### Performance Issues
- Try reducing buffer size: `--buffer-size 10000`
- Use pod sampling: `--sample 5`
- Check available system resources

### UI/TUI Issues
- Update your terminal: Use a modern terminal emulator
- Check terminal size: Ensure minimum 80x24 characters
- For macOS: Use [iTerm2](https://iterm2.com/) for best experience

## Feature Requests

Have an idea for a new feature? We'd love to hear it!

1. Check existing [feature requests](https://github.com/samba-rgb/wake/issues?q=is%3Aissue+is%3Aopen+label%3Aenhancement)
2. Create a new issue with the `enhancement` label
3. Describe your use case and why it would be helpful

## Contributing

Want to contribute to Wake?

1. Check the [contribution guidelines](https://github.com/samba-rgb/wake/blob/main/CONTRIBUTING.md)
2. Look for [good first issues](https://github.com/samba-rgb/wake/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
3. Fork the repository and submit a pull request

## Show Your Support

If you like the application, please consider giving it a star on GitHub — it really helps the project:

⭐ [Star Wake on GitHub](https://github.com/samba-rgb/wake)

## Community

- **GitHub Discussions**: [Join the community](https://github.com/samba-rgb/wake/discussions)
- **Documentation**: [Read the full docs](https://wakelog.in)
- **Releases**: [Stay updated](https://github.com/samba-rgb/wake/releases)
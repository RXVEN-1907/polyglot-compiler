# Security Policy

## Supported Versions

We release security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | ✅ Yes             |
| < 0.1   | ❌ No              |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security vulnerability in Polyglot, please report it responsibly.

### How to Report

**Please do NOT create a public GitHub issue for security vulnerabilities.**

Instead, email us at: **security@polyglot-compiler.dev**

Or use GitHub's private vulnerability reporting:
1. Go to the [Security Advisories](https://github.com/RXVEN-1907/polyglot-compiler/security/advisories) tab
2. Click "Report a vulnerability"
3. Fill in the details

### What to Include

Please include the following information:

- **Description**: A clear description of the vulnerability
- **Impact**: What an attacker could achieve
- **Reproduction Steps**: Minimal steps to reproduce the issue
- **Affected Versions**: Which versions are affected
- **Suggested Fix**: If you have ideas for mitigation

### Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 5 business days
- **Fix Development**: Depends on severity (typically 2-4 weeks)
- **Disclosure**: Coordinated with reporter

## Security Considerations

### Compiler Driver Security

Polyglot is a compiler driver that:
- Executes external commands (clang, clang++)
- Processes user-provided source code
- Reads configuration files (`polyglot.toml`)
- Creates temporary files during compilation

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Command injection via file paths | Input validation, shell=False in subprocess |
| Path traversal in config | Path normalization, directory validation |
| Malicious source code | Sandboxed execution, no code execution during compile |
| Supply chain attacks | Dependency pinning, minimal dependencies |

### Best Practices for Users

1. **Verify downloads**: Check checksums of released binaries
2. **Isolate builds**: Run untrusted code in containers/VMs
3. **Keep updated**: Regularly update Polyglot and Clang/LLVM
4. **Review configs**: Audit `polyglot.toml` from untrusted sources

### Dependency Security

We monitor dependencies for vulnerabilities using:
- GitHub Dependabot alerts
- `cargo audit` for Rust dependencies
- `pip audit` for Python dependencies

To audit locally:
```bash
# Rust dependencies
cargo install cargo-audit
cargo audit

# Python dependencies
pip install pip-audit
pip audit
```

## Security-Related Configuration

### Safe Defaults

- No arbitrary code execution during compilation
- Temporary files use secure random names
- No network access during compilation (unless explicitly configured)
- Subprocess execution with explicit arguments (no shell)

### Hardening Options

For production use, consider:
```bash
# Restrict compiler capabilities
polyglot build --opt-level size untrusted.c -o out

# Use isolated build directory
polyglot build --output /tmp/build/out untrusted.c
```

## Vulnerability Disclosure Process

1. **Report Received**: Acknowledgment within 48 hours
2. **Triage**: Severity assessment (CVSS scoring)
3. **Investigation**: Root cause analysis
4. **Fix Development**: Patch creation and testing
5. **Release**: Security advisory and patched version
6. **Coordination**: Work with reporter on disclosure timing

## Contact

- **Security Email**: security@polyglot-compiler.dev
- **PGP Key**: Available on request
- **Bug Bounty**: Not currently offered

---

*Last updated: 2024*
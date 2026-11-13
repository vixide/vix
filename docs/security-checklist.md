# Security Checklist for New Features

This checklist helps maintainers and contributors ensure that new features
and changes to Vix maintain the project's high security standards.

## Before Implementation

- [ ] **Review SECURITY.md** - Understand Vix's security policy and reporting process
- [ ] **Check for sensitive data** - Identify if the feature will handle:
  - [ ] User credentials or secrets
  - [ ] File system access
  - [ ] Network requests
  - [ ] Process execution
  - [ ] Environment variables
  - [ ] User input that could be malicious
- [ ] **Assess risk level** - Low, Medium, or High based on potential impact
- [ ] **Design with least privilege** - Ensure the feature has only the permissions it needs

## During Implementation

### Code Safety

- [ ] **No `unsafe` code** - Vix forbids `unsafe` Rust in all crates
- [ ] **Use safe APIs** - Prefer standard library and well-audited crates
- [ ] **Input validation** - All external inputs are validated:
  - [ ] Command-line arguments
  - [ ] File contents
  - [ ] User input from UI
  - [ ] Network data
  - [ ] Environment variables
- [ ] **Bounds checking** - No unchecked array/file/buffer access
- [ ] **Error handling** - All errors are handled gracefully without panic
- [ ] **Memory safety** - No memory leaks, use-after-free, or buffer overflows

### Data Handling

- [ ] **Sensitive data protection** - Secrets are:
  - [ ] Never logged
  - [ ] Never stored in plaintext
  - [ ] Masked in UI
  - [ ] Cleared from memory when done
- [ ] **File access** - Files are:
  - [ ] Opened with appropriate permissions
  - [ ] Validated before access
  - [ ] Never written to unintended locations
  - [ ] Properly closed after use
- [ ] **Network access** - Network requests are:
  - [ ] Made only to user-specified endpoints
  - [ ] Validated for scheme (HTTP/HTTPS only where appropriate)
  - [ ] Timeout-protected
  - [ ] Rate-limited where applicable

### Execution Safety

- [ ] **Process execution** - Commands are:
  - [ ] Never executed with user input without validation
  - [ ] Run with appropriate working directory
  - [ ] Run with appropriate environment
  - [ ] Timeout-protected
- [ ] **Shell injection prevention** - No user input in shell commands without proper escaping
- [ ] **Script sandboxing** - Rhai scripts:
  - [ ] Have no file access by default
  - [ ] Have no network access by default
  - [ ] Have no process execution by default
  - [ ] Are explicitly opted-in for additional capabilities

### Dependency Security

- [ ] **Use trusted dependencies** - Only well-maintained, audited crates
- [ ] **Check for vulnerabilities** - Run `cargo deny check` and `cargo audit`
- [ ] **Minimize dependencies** - Only include what's necessary
- [ ] **Pin versions** - Use exact versions, not wildcards
- [ ] **Review licenses** - Ensure compatibility with Vix's multi-license approach

## After Implementation

### Code Review

- [ ] **Security-focused review** - At least one reviewer with security expertise
- [ ] **Threat modeling** - Consider potential attack vectors:
  - [ ] What if user input is malicious?
  - [ ] What if files are tampered with?
  - [ ] What if network responses are malicious?
  - [ ] What if dependencies are compromised?
- [ ] **Penetration testing** - Manual testing for:
  - [ ] Injection attacks
  - [ ] Path traversal
  - [ ] Denial of service
  - [ ] Privilege escalation
  - [ ] Information disclosure

### Testing

- [ ] **Security-specific tests** - Add tests for:
  - [ ] Input validation
  - [ ] Error cases
  - [ ] Edge cases
  - [ ] Boundary conditions
- [ ] **Fuzz testing** - Add fuzz targets for new parsers or complex input handling
- [ ] **Integration testing** - Test with various configurations and inputs

### Documentation

- [ ] **Document security implications** - In:
  - [ ] Feature specification (`spec/.../index.md`)
  - [ ] User documentation (`docs/.../index.md`)
  - [ ] Code comments
- [ ] **Document security assumptions** - What the feature assumes about its environment
- [ ] **Document security limitations** - What the feature does NOT protect against

## Before Release

### Security Audit

- [ ] **Run automated checks**:
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings`
  - [ ] `cargo fmt --check`
  - [ ] `cargo test --workspace`
  - [ ] `cargo deny check`
  - [ ] `scripts/check` (full tier)
- [ ] **Manual review** - Check for:
  - [ ] Hardcoded secrets or credentials
  - [ ] Insecure defaults
  - [ ] Missing input validation
  - [ ] Missing error handling
  - [ ] Information leaks in error messages

### Vulnerability Assessment

- [ ] **Check for known vulnerabilities** - Use `cargo audit`
- [ ] **Check supply chain** - Review dependency tree for suspicious packages
- [ ] **Check for regressions** - Ensure existing security features still work

## Post-Release

### Monitoring

- [ ] **Monitor for issues** - Watch for security-related bug reports
- [ ] **Respond promptly** - To any security concerns or vulnerability reports
- [ ] **Disclose responsibly** - Follow SECURITY.md for vulnerability disclosure

### Maintenance

- [ ] **Update dependencies** - Keep dependencies up to date with security patches
- [ ] **Review security posture** - Regularly reassess feature security
- [ ] **Deprecate if needed** - Remove features with unfixable security issues

---

## Security Design Principles

### 1. Defense in Depth

Apply multiple layers of security controls. Don't rely on a single mechanism.

### 2. Least Privilege

Grant the minimum permissions necessary for a feature to function.

### 3. Fail Secure

When errors occur, fail in a secure state rather than exposing sensitive data or allowing unintended access.

### 4. Secure by Default

Security should be enabled by default, not an opt-in feature.

### 5. Input Validation

All external inputs must be validated before use. Never trust user input.

### 6. Output Encoding

When displaying data, encode it appropriately to prevent injection attacks.

### 7. Minimize Attack Surface

Reduce the number of entry points and features that could be exploited.

### 8. Keep Security Simple

Complex security mechanisms are more likely to have vulnerabilities.

---

## Common Security Pitfalls

### ❌ To Avoid

- **Hardcoded secrets** - Never commit credentials, API keys, or passwords
- **Unvalidated input** - Always validate before processing
- **Unsanitized output** - Always encode before displaying
- **Race conditions** - Use proper synchronization
- **TOCTOU (Time of Check to Time of Use)** - Validate at the time of use, not before
- **Insecure deserialization** - Use safe deserialization methods
- **Command injection** - Never include user input in shell commands
- **Path traversal** - Validate and sanitize file paths
- **Buffer overflows** - Use safe Rust types and bounds checking
- **Integer overflows** - Use checked arithmetic for security-critical operations

### ✅ To Do

- **Use Rust's type system** - Leverage Rust's safety features
- **Use well-audited crates** - Prefer mature, widely-used dependencies
- **Validate all inputs** - Assume all external data is malicious
- **Handle all errors** - Don't panic, handle gracefully
- **Use secure defaults** - Make safe choices the default
- **Document security decisions** - Explain why certain approaches were chosen

---

## Security Resources

- [Vix Security Policy](../SECURITY.md) - Reporting vulnerabilities
- [Rust Security Guidelines](https://rust-lang.org/security) - Rust language security
- [OWASP Top 10](https://owasp.org/www-project-top-ten/) - Web application security
- [CWE Top 25](https://cwe.mitre.org/top25/) - Common Weakness Enumeration
- [SANS Secure Coding](https://www.sans.org/blog/secure-coding-principles/) - Secure coding principles

---

## Quick Reference: Security Review Checklist

Before merging any PR that touches security-sensitive code:

- [ ] No `unsafe` code added
- [ ] All inputs validated
- [ ] All errors handled
- [ ] No hardcoded secrets
- [ ] No shell injection vulnerabilities
- [ ] No path traversal vulnerabilities
- [ ] Dependencies are trusted and up-to-date
- [ ] Feature follows least privilege principle
- [ ] Feature fails securely
- [ ] Security implications documented

---

Vix™ and Vix IDE™ are trademarks.

# Security policy

J3 is alpha software (0.x). Only the latest release gets fixes.

## Reporting a vulnerability

Report privately through GitHub: **Security** tab, **Report a vulnerability**
(<https://github.com/Jacoby-Y/j3/security/advisories/new>). Please do not open a public issue for it.
Include the `j3c version --verbose` output, a small repro, and what an attacker gains.

## In scope

- The compiler `j3c`: crashes or code execution from a crafted source file, project, `j3.toml`, scene or asset.
- `j3.net` (TCP, DNS, TLS on the system OpenSSL, HTTP): certificate checks, parsing of server replies.
- j2 runtime scripts (`j3.j2`): a mod script escaping the API the host game gives it.
- `install.sh` and the release packages.

Out of scope: games built with J3 (report to their authors), and bugs that need an attacker who can
already run code as you.

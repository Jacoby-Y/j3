# Packaging

Every channel ships the same `j3c` binary, built by `.github/workflows/release.yml` on a
`v*` tag with `tools/release/package.sh`. `j3c-composer` is not packaged yet (separate tool;
`./build.sh` installs it from source).

| Channel | Files | Per release |
|---|---|---|
| Tarball + `install.sh` | `j3c-<ver>-<arch>-linux.tar.gz`, `SHA256SUMS` | Automatic: the workflow uploads them; publish the draft release. `install.sh` picks up the latest published one |
| `.deb` | `j3c_<ver>_<amd64\|arm64>.deb` | Automatic, same release. `sudo apt install ./j3c_<ver>_amd64.deb`. A PPA or apt repo is later |
| AUR `j3c-bin` | `aur/j3c-bin/PKGBUILD` | After publishing: bump `pkgver`, `updpkgsums`, `makepkg --printsrcinfo > .SRCINFO`, push both to `ssh://aur@aur.archlinux.org/j3c-bin.git` |
| Homebrew, winget/Scoop, npm | | Later, once macOS/Windows builds run |

Release steps: bump `version` in `j3c/Cargo.toml`, move the `CHANGELOG.md` section from
unreleased to a date, commit, tag `v<ver>`, push the tag. The workflow refuses a tag that
does not match `Cargo.toml`.

## Compatibility

- Linux x86_64 and aarch64 only.
- glibc 2.35 or newer (built on Ubuntu 22.04): Ubuntu 22.04+, Debian 12+, Fedora 36+, any
  current Arch. The `.deb` states the exact floor in `Depends: libc6 (>= ...)`.
- Needs `clang` at run time to build programs; SDL2, libGL, and fonts are recommended (the
  `.deb` Recommends them; `j3c doctor` checks them).

## Uninstall

- `install.sh`: `rm ~/.local/bin/j3c` (or your `J3_INSTALL_DIR`); caches are in `~/.cache/j3`.
- `.deb`: `sudo apt remove j3c`. AUR: `pacman -R j3c-bin`.

Upgrade: run `install.sh` again (or `J3_VERSION=<ver>` for a given one), install the new `.deb`, or update the AUR package.

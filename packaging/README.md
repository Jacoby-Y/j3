# Packaging

Every channel ships the same `j3c` binary, built by `.github/workflows/release.yml` on a
`v*` tag with `tools/release/package.sh`. `j3c-composer` is not packaged yet (separate tool;
`./build.sh` installs it from source).

| Channel | Files | Per release |
|---|---|---|
| Tarball + `install.sh` | `j3c-<ver>-<arch>-linux.tar.gz`, `SHA256SUMS` | Automatic: the workflow uploads them; publish the draft release. `install.sh` picks up the latest published one |
| `.deb` | `j3c_<ver>_<amd64\|arm64>.deb` | Automatic, same release. `sudo apt install ./j3c_<ver>_amd64.deb`. A PPA or apt repo is later |
| AUR `j3c-bin` | `aur/j3c-bin/PKGBUILD` | After publishing: bump `pkgver`, `updpkgsums`, `makepkg --printsrcinfo > .SRCINFO`, push both to `ssh://aur@aur.archlinux.org/j3c-bin.git` |
| Windows zip + `install.ps1` | `j3c-<ver>-x86_64-windows.zip` (in `SHA256SUMS`), `install.ps1` | Automatic: the x86_64 job cross-builds `j3c.exe` (`package.sh --windows`) and uploads both. `install.ps1` takes the latest published release |
| Scoop | `scoop/j3c.json` | After publishing: `checkver.ps1 -App j3c -Dir packaging/scoop -Update` (from a Scoop checkout) fills `version`, `url` and `hash` (the zeros until the first Windows release); commit. Users install from the file's URL: `scoop install https://raw.githubusercontent.com/Jacoby-Y/j3/main/packaging/scoop/j3c.json` (a bucket repo, for `scoop update`, is later) |
| winget | `winget/Jacoby-Y.j3c*.yaml` | Skeleton only. After the second Windows release (winget wants a URL history): `wingetcreate update Jacoby-Y.j3c --version <ver> --urls <zip url>` and submit to `microsoft/winget-pkgs`. The portable install does not run `j3c setup`: the first build says to |
| Homebrew, npm | | Later, once macOS builds run |

Release steps: move the `CHANGELOG.md` section from unreleased to a date, commit, tag
`v<ver>`, push the tag. No version bump: the version is `git describe --tags --match 'v[0-9]*'`
(`0.1.4` on the tag, `0.1.4-7-gabc1234[-dirty]` after it), which `./build.sh` and
`tools/release/package.sh` write to the gitignored `j3c-bootstrap/text/version-build.txt` for
the build (`src/project/config.j3`: `j3cBuild`, `j3cSemver`; without the file it is
`J3C_VERSION`, 0.1.0). The workflow refuses a binary whose version is not the tag.

## Compatibility

- Linux x86_64 and aarch64; Windows 10 1903+ / 11 x64 (the zip; `j3c setup` downloads llvm-mingw and the SDL2
  kit into `%LOCALAPPDATA%\j3`; unsigned, so SmartScreen asks once). The Windows host is unverified on real
  hardware (plan 08 chunk 10).
- glibc 2.35 or newer (built on Ubuntu 22.04): Ubuntu 22.04+, Debian 12+, Fedora 36+, any
  current Arch. The `.deb` states the exact floor in `Depends: libc6 (>= ...)`.
- Needs `clang` at run time to build programs; SDL2, libGL, and fonts are recommended (the
  `.deb` Recommends them; `j3c doctor` checks them).

## Uninstall

- `install.sh`: `rm ~/.local/bin/j3c` (or your `J3_INSTALL_DIR`); caches are in `~/.cache/j3`.
- `.deb`: `sudo apt remove j3c`. AUR: `pacman -R j3c-bin`.
- Windows: delete `%LOCALAPPDATA%\Programs\j3c` and `%LOCALAPPDATA%\j3`, and the PATH entry (Settings, "Edit environment variables for your account"). Scoop: `scoop uninstall j3c` (then `%LOCALAPPDATA%\j3`).

Upgrade: run `install.sh` again (or `J3_VERSION=<ver>` for a given one), install the new `.deb`, or update the AUR package.

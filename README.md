<p align="center"><img src="logo.svg" width="96" alt="J3 logo"></p>

# J3

The best language/engine for making cross platform games/applications. It's blazingly fast in runtime and build speeds, makes beautiful UI using HTML/CSS sugar (via .j3ui), and has extensive docs for you and your agent.

Don't believe me? There's a game publishing to Steam soon, made entirely in J3! (Will update with more info when public)

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Jacoby-Y/j3/main/install.sh | sh
```

Or grab the `.deb` from [Releases](https://github.com/Jacoby-Y/j3/releases), or `yay -S j3c-bin` on Arch.

You also need clang and SDL2. `j3c doctor` tells you what's missing. On Ubuntu/Debian:

```sh
sudo apt install clang libsdl2-2.0-0 libsdl2-ttf-2.0-0 libsdl2-image-2.0-0 libsdl2-mixer-2.0-0 libgl1
```

From source (needs Rust + clang): `git clone https://github.com/Jacoby-Y/j3 && cd j3 && ./build.sh`

## Start

```sh
j3c new hello --app
cd hello
j3c run
```

`j3c guide` for the rest, `j3c doc` for the stdlib, `j3c doc recipes` for full example programs.

## Examples

In [`examples/`](examples). Run one with `j3c run examples/<name>`.

## License

MIT or Apache-2.0. Third-party assets: [THIRD_PARTY.md](THIRD_PARTY.md).

# Cross-compiling locally with `cross`

> **What this is.** An optional local-QA recipe for building and smoke-testing BioRouter release binaries for other architectures on your own machine, using the [`cross`](https://github.com/cross-rs/cross) tool.
> **Status:** Current.
> **Audience:** developers doing ad-hoc packaging or architecture QA.

`cross` wraps `cargo build` in a per-target Docker container so you can produce a
Linux or Intel-macOS binary from, say, an Apple Silicon laptop. This page covers the
four targets that come up in practice, plus how to run the resulting binary inside a
matching container so you can actually exercise it.

> **Note.** This is *not* how releases are cut. The shipped pipeline
> ([`scripts/release.sh`](../../scripts/release.sh) and the `release-*` targets in the `Justfile`)
> cross-compiles by invoking `docker run` + `cargo build` directly and never calls `cross`. Use this
> guide only for ad-hoc local testing of a target the release matrix doesn't cover.

## Prerequisites

Before you start, check the comments in [`Cross.toml`](../../Cross.toml) and uncomment the
configuration for the target you want to build.

## Linux targets

The procedure is the same for both Linux targets; only the target triple and the
test container image differ.

| Target triple | Test image | `docker run --platform` |
|---|---|---|
| `aarch64-unknown-linux-gnu` | `arm64v8/ubuntu` | `linux/arm64` |
| `x86_64-unknown-linux-gnu` | `ubuntu:latest` | `linux/amd64` |

### Build release

For `aarch64-unknown-linux-gnu`:

```sh
CROSS_BUILD_OPTS="--platform linux/amd64 --no-cache" CROSS_CONTAINER_OPTS="--platform linux/amd64" cross build --release --target aarch64-unknown-linux-gnu
```

For `x86_64-unknown-linux-gnu`:

```sh
CROSS_BUILD_OPTS="--platform linux/amd64 --no-cache" CROSS_CONTAINER_OPTS="--platform linux/amd64" cross build --release --target x86_64-unknown-linux-gnu
```

### Inspect the container created by cross for debugging

```sh
docker run --platform linux/amd64 -it <image-id> /bin/bash
```

### Test the binary

1. Download the docker image for the testing environment.

   For `aarch64-unknown-linux-gnu`:

   ```sh
   docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
   docker pull arm64v8/ubuntu
   ```

   For `x86_64-unknown-linux-gnu`:

   ```sh
   docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
   docker pull --platform linux/amd64 ubuntu:latest
   ```

2. Run the container. `pwd` is the directory on your host machine that contains the
   binary built in the previous step.

   For `aarch64-unknown-linux-gnu`:

   ```sh
   docker run --rm -v "$(pwd)":/app -it --platform linux/arm64 arm64v8/ubuntu /bin/bash
   ```

   For `x86_64-unknown-linux-gnu`:

   ```sh
   docker run --rm -v "$(pwd)":/app -it --platform linux/amd64 ubuntu:latest /bin/bash
   ```

3. Install dependencies in the container and set up the API testing environment.
   The last two steps are left to you: write a config file at
   `~/.config/biorouter/config.yaml` (see the
   [config file reference](../configuration/config-file-reference.md)) and export your
   provider's API key (see [environment variables](../configuration/environment-variables.md)).

   ```sh
   apt update
   apt install libxcb1 libxcb1-dev libdbus-1-3 nvi
   mkdir -p ~/.config/biorouter
   # create biorouter config file
   # set api key env variable
   ```

## macOS targets

There is no docker image available for either macOS target. `cross` falls back to
your host machine for building the binary if your host machine matches.

### aarch64-apple-darwin

#### Build release

```sh
cross build --release --target aarch64-apple-darwin
```

#### Test the build

If the binary is signed with a certificate, run:

```sh
xattr -d com.apple.quarantine biorouter
```

### x86_64-apple-darwin

#### Build release

```sh
cross build --release --target x86_64-apple-darwin
```

#### Test the build

1. If the binary is signed with a certificate, run:

   ```sh
   xattr -d com.apple.quarantine biorouter
   ```

2. If you are on Apple Silicon (ARM), you can use Rosetta to test the binary.

   ```sh
   softwareupdate --install-rosetta # make sure Rosetta 2 is installed
   ```

   ```sh
   arch -x86_64 ./biorouter help
   ```

## Related documentation

- [Auto-update test checklist](auto-update-test-checklist.md) — sibling release-process doc covering the per-release macOS update QA.
- [Headless Linux deployment](../deployment/headless-linux.md) — where a cross-built Linux binary actually gets used.
- [Config file reference](../configuration/config-file-reference.md) — the `config.yaml` you need to write inside the test container.
- [Environment variables](../configuration/environment-variables.md) — the API-key and other variables to set for a smoke test.

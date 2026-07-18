# Cross-compiling locally with `cross`

Optional local-QA path for building release binaries for other architectures on your own machine.

> **Note:** this is *not* how releases are cut. The shipped pipeline
> ([`scripts/release.sh`](../../scripts/release.sh) and the `release-*` targets in the `Justfile`)
> cross-compiles by invoking `docker run` + `cargo build` directly and never calls `cross`. Use this
> guide only for ad-hoc local testing of a target the release matrix doesn't cover.

## Prerequisites
Before start, check the comments in `Cross.toml` to turn on some commented configs for the target you want to build.

## Targets
### aarch64-unknown-linux-gnu

#### Build release
```sh   
CROSS_BUILD_OPTS="--platform linux/amd64 --no-cache" CROSS_CONTAINER_OPTS="--platform linux/amd64" cross build --release --target aarch64-unknown-linux-gnu
```

#### Inspect container created by cross for debugging
```sh 
docker run --platform linux/amd64 -it <image-id> /bin/bash
```

#### Testing the binary

1. Download docker image for testing environment
```sh
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
docker pull arm64v8/ubuntu
```
2. Run the container
pwd is the directory which contains the binary built in the previous step on your host machine
```sh
docker run --rm -v "$(pwd)":/app -it --platform linux/arm64 arm64v8/ubuntu /bin/bash
```

3. Install dependencies in the container and set up api testing environment
```sh 
apt update
apt install libxcb1 libxcb1-dev libdbus-1-3 nvi
mkdir -p ~/.config/biorouter
# create biorouter config file
# set api key env variable
```

### x86_64-unknown-linux-gnu

#### Build release
```sh   
CROSS_BUILD_OPTS="--platform linux/amd64 --no-cache" CROSS_CONTAINER_OPTS="--platform linux/amd64" cross build --release --target x86_64-unknown-linux-gnu
```
#### inspect container created by cross for debugging
```sh 
docker run --platform linux/amd64 -it <image-id> /bin/bash
```

#### Testing the binary

1. Download docker image for testing environment
```sh
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
docker pull --platform linux/amd64 debian:latest
```

2. Run the container
pwd is the directory contains the binary built in the previous step on your host machine
```sh
docker run --rm -v "$(pwd)":/app -it --platform linux/amd64 ubuntu:latest /bin/bash
```

3. Install dependencies in the container and set up api testing environment
```sh 
apt update
apt install libxcb1 libxcb1-dev libdbus-1-3 nvi
mkdir -p ~/.config/biorouter
# create biorouter config file
# set api key env variable
```

### aarch64-apple-darwin

#### build release
```sh   
cross build --release --target aarch64-apple-darwin
```
There is no docker image available for aarch64-apple-darwin. It will fall back to your host machine for building the binary if your host machine matches.

#### testing the build
If the binary is signed with a certificate, run
```sh
xattr -d com.apple.quarantine biorouter
````

### x86_64-apple-darwin

#### build release
```sh   
cross build --release --target x86_64-apple-darwin
```

There is no docker image available for x86_64-apple-darwin. It will fall back to your host machine for building the binary if your host machine matches.

#### testing the build
1. If the binary is signed with a certificate, run
```sh
xattr -d com.apple.quarantine biorouter
````
2. If you are on Apple Silicon (ARM), you can use Rosetta to test the binary
```sh
softwareupdate --install-rosetta # make sure Rosetta 2 is installed
```

```sh
arch -x86_64 ./biorouter help
```

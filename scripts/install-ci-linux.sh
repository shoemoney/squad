#!/usr/bin/env bash
# Run on the chosen Linux worker. Read a short-lived registration token on stdin.
set -euo pipefail
set +x
umask 077
if [[ ${1:-} == --help ]]; then
    echo 'Usage: registration-token-command | ./scripts/install-ci-linux.sh IMAGE_DIGEST'
    echo 'Installs one Squad runner with an internal Docker daemon, 4 CPUs and 16 GiB RAM.'
    exit 0
fi
base_image=${1:?Pass a pinned myoung34/github-runner image digest.}
[[ $base_image == myoung34/github-runner@sha256:* ]] || { echo 'Expected a pinned runner image digest.' >&2; exit 2; }
IFS= read -r runner_token || { echo 'Missing registration token on stdin.' >&2; exit 2; }
[[ -n $runner_token ]] || { echo 'Missing registration token on stdin.' >&2; exit 2; }
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if docker container inspect squad-ci-linux >/dev/null 2>&1; then
    echo 'squad-ci-linux already exists; inspect it before replacing an active runner.' >&2
    exit 1
fi
docker build --build-arg "BASE_IMAGE=$base_image" -f "$root/infra/ci/Dockerfile.linux" -t squad-ci-runner:local "$root/infra/ci"
env_file=$(mktemp)
trap 'rm -f "$env_file"' EXIT
cat > "$env_file" <<ENV
REPO_URL=https://github.com/shoemoney/squad
RUNNER_NAME=squad-ci-linux
RUNNER_SCOPE=repo
LABELS=squad-ci
RUNNER_WORKDIR=/home/runner/work
CONFIGURED_ACTIONS_RUNNER_FILES_DIR=/runner-state
DISABLE_AUTOMATIC_DEREGISTRATION=true
UNSET_CONFIG_VARS=true
START_DOCKER_SERVICE=true
RUN_AS_ROOT=false
CARGO_BUILD_JOBS=4
CARGO_TARGET_DIR=/home/runner/target/squad
ENV
printf 'RUNNER_TOKEN=%s\n' "$runner_token" >> "$env_file"
unset runner_token
# Docker runs inside this container. No host Docker socket, ports, home or keys.
# One runner serializes all Linux jobs; the outer limit also bounds nested builds.
docker run -d --name squad-ci-linux --restart unless-stopped \
    --privileged --cpus 4 --memory 16g --memory-swap 16g \
    --env-file "$env_file" \
    -v squad-ci-state:/runner-state \
    -v squad-ci-work:/home/runner/work \
    -v squad-ci-cargo:/home/runner/.cargo \
    -v squad-ci-rustup:/home/runner/.rustup \
    -v squad-ci-target:/home/runner/target \
    -v squad-ci-docker:/var/lib/docker \
    squad-ci-runner:local

# Automatic CI on project machines

Every push runs `.github/workflows/ci.yml`. The same workflow also runs for pull
requests and manual dispatch. Checks remain visible in GitHub and keep their
existing names. There is no inbound webhook or public listener to configure:
the Actions runner connects outbound to GitHub.

## Routing

Repository variables select the workers for pushes and manual runs:

| Variable | Value |
|---|---|
| `CI_LINUX_RUNNER` | `["self-hosted","Linux","X64","squad-ci"]` |
| `CI_MACOS_RUNNER` | `["self-hosted","macOS","ARM64","squad-ci"]` |

The Rust and installer jobs keep their `ubuntu-latest` / `macos-latest` matrix
names. Those names describe the compatibility leg; the variables select its
actual worker. Shellcheck and Debian/Fedora installer containers use Linux.
Deleting either variable returns that platform to GitHub-hosted runners.

**All pull-request events use hosted runners**, including same-repository PRs.
Set the repository's fork contributor approval policy to
`all_external_contributors`. Review workflow changes before approving external
runs: a PR can modify its own workflow, including runner labels. Do not enable
`pull_request_target` to execute a contributor's checkout on these workers.

## Linux worker

One persistent runner, `squad-ci-linux`, serializes Linux jobs. It runs in a
Docker container with four CPUs, 16 GiB RAM, no extra swap, and automatic restart.
It starts a dedicated nested Docker daemon for the Debian/Fedora job containers.
The runner container requires privileged mode for this daemon. It has **no host
Docker socket, development home, credentials directory or published port**.
A job-start hook restores workspace ownership after installer containers change
it to their own user, before the next checkout.
It uses dedicated named volumes for registration state, work, Cargo, Rustup,
build outputs and its own Docker data. These are CI caches, not production data.

Build the runner image from `infra/ci/Dockerfile.linux`, using a reviewed pinned
`myoung34/github-runner:ubuntu-noble` digest. The deployed base is:

```text
myoung34/github-runner@sha256:332eb773befd8ae3ec7cf1370e31a3bf9b1e0b91c3f4aa70939e7fa078c234c1
```

On the selected Linux worker, with Docker installed and this repository checked
out, feed a short-lived repository registration token to the installer on stdin:

```bash
gh api -X POST repos/shoemoney/squad/actions/runners/registration-token --jq .token |
  ./scripts/install-ci-linux.sh myoung34/github-runner@sha256:332eb773befd8ae3ec7cf1370e31a3bf9b1e0b91c3f4aa70939e7fa078c234c1
```

The installer temporarily holds the registration token in a private env file,
removes that file on exit, and removes configuration variables from the runner's
job environment. It does not require or retain a personal access token. An
existing container causes installation to stop rather than replacing a busy job.

Status and logs, on that worker:

```bash
docker ps --filter name=squad-ci-linux
docker logs --tail 100 squad-ci-linux
docker exec -u runner squad-ci-linux docker info
```

Stop it with `docker stop squad-ci-linux`. Start it with
`docker start squad-ci-linux`; saved registration is reused. Remove the GitHub
runner registration separately when retiring it. Do not remove its cache volumes
while it is running, and do not prune the host's shared Docker data.

## ARM Mac worker

One runner, `squad-ci-macos`, runs as hidden user `squad-ci`, with its own private
home at `/Users/squad-ci`, no password login, and no administrator membership.
It uses Homebrew's existing tools and the installed Xcode command-line tools.
Install those prerequisites as the administrator before registering the worker;
the CI account must not need administrator access to run `setup.sh --no-build`.
Cargo/Rustup and build caches belong to the CI account. Cargo uses four workers.

Download the official macOS ARM64 runner archive and its SHA256 from GitHub's
`repos/shoemoney/squad/actions/runners/downloads` API. Supply the local archive
path and published checksum to the installer, with a registration token on stdin:

```bash
gh api -X POST repos/shoemoney/squad/actions/runners/registration-token --jq .token |
  sudo -n ./scripts/install-ci-macos.sh /path/to/official-runner.tar.gz SHA256
```

The installer verifies the checksum before extraction and registers the runner
without putting the token in command arguments. It creates launch daemon
`ai.shoemoney.squad-ci`, which starts at boot and restarts after an unexpected
exit. The service runs at reduced scheduling priority. Installers refuse to
replace an existing registration or service.

Status and logs, on the Mac:

```bash
sudo launchctl print system/ai.shoemoney.squad-ci
sudo tail -100 /Users/squad-ci/logs/runner.log
sudo tail -100 /Users/squad-ci/logs/runner.err
```

Stop it with `sudo launchctl bootout system/ai.shoemoney.squad-ci`; start it with
`sudo launchctl bootstrap system /Library/LaunchDaemons/ai.shoemoney.squad-ci.plist`.

## Checks and credentials

All Rust checks use `rust-toolchain.toml` and the committed lockfile: formatting,
Clippy with warnings denied, build, workspace tests, Nextest and documentation
tests. Installer fixtures, shellcheck and full Debian/Fedora installs also run.
`CARGO_BUILD_JOBS=4` reaches both native builds and installer subprocesses.
`RUST_TEST_THREADS=1` and `NEXTEST_TEST_THREADS=1` serialize test fixtures.
Each room fixture already starts multiple workers; concurrent fixtures can
exceed their bounded lifecycle deadlines on the four-CPU Linux runner.
Jobs on each runner execute sequentially; newer pushes cancel older non-main,
non-tag runs of the same branch.

CI uses loopback model fixtures and an empty API key. Container installer checks
use a dummy key. No live OpenRouter key or real `.env` belongs on either worker.
Registration tokens must never be printed, committed or passed on command lines.
Keep runner credential files private; do not include them in support archives.

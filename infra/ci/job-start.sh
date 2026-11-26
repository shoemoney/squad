#!/usr/bin/env bash
set -euo pipefail
# Installer containers chown their checkout to uid 1000. Restore the runner's
# ownership before checkout reuses that same persistent work volume.
sudo chown -hR -- "$(id -u):$(id -g)" /home/runner/work

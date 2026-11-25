#!/usr/bin/env bash
set -euo pipefail
# Credentials must remain private even if the caller enables shell tracing.
set +x

usage() {
    cat <<'HELP'
Usage: ./setup.sh [--yes] [--runtime-only] [--no-build] [--help]

Supported platforms: macOS arm64 (Homebrew), Linux x64 (apt-get or dnf).
Ensures a C toolchain, pkg-config, git and curl; installs rustup and the
repository's pinned Rust toolchain with clippy and rustfmt; installs
cargo-nextest and cargo-insta; creates a private .env only if absent;
reports credential presence; builds the locked release and runs squad doctor.

  --yes           Noninteractive; permits printed installer commands unattended
  --runtime-only  Skip cargo-nextest and cargo-insta
  --no-build      Skip release build and built-binary diagnostics
  --help          Show this help without making changes
HELP
}

yes=false
runtime_only=false
no_build=false
for arg in "$@"; do
    case "$arg" in
        --yes) yes=true ;;
        --runtime-only) runtime_only=true ;;
        --no-build) no_build=true ;;
        --help) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$arg" >&2; usage >&2; exit 2 ;;
    esac
done

die() { printf 'Setup: %s\n' "$*" >&2; exit 1; }
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd -- "$root"

privileged() {
    local command=()
    if [[ $(id -u) != 0 ]]; then
        command=(sudo)
        if "$yes"; then
            command+=(-n)
        elif [[ ! -t 0 ]]; then
            die 'Unattended sudo requires --yes. Re-run with --yes or use a terminal.'
        fi
    elif ! "$yes" && [[ ! -t 0 ]]; then
        die 'Unattended installation as root requires --yes. Re-run with --yes or use a terminal.'
    fi
    command+=("$@")
    printf 'Running:'
    printf ' %q' "${command[@]}"
    printf '\n'
    "${command[@]}"
}

os=$(uname -s)
arch=$(uname -m)
case "$os/$arch" in
    Darwin/arm64)
        manager=brew
        command -v brew >/dev/null || die 'Install Homebrew (https://brew.sh), then re-run setup.'
        ;;
    Linux/x86_64)
        if command -v apt-get >/dev/null; then
            manager=apt-get
        elif command -v dnf >/dev/null; then
            manager=dnf
        else
            die 'Linux x64 requires apt-get or dnf.'
        fi
        ;;
    *) die "Unsupported platform: $os/$arch. Supported: macOS arm64 and Linux x64 (apt-get or dnf)." ;;
esac
printf 'Platform: %s/%s; package manager: %s\n' "$os" "$arch" "$manager"

if [[ $manager == brew ]]; then
    if ! xcode-select -p >/dev/null 2>&1; then
        xcode-select --install
        die 'Complete the Xcode Command Line Tools installation, then re-run setup.'
    fi
    missing=()
    for tool in pkg-config git curl; do
        command -v "$tool" >/dev/null || missing+=("$tool")
    done
    if ((${#missing[@]})); then
        HOMEBREW_NO_AUTO_UPDATE=1 brew install "${missing[@]}"
    fi
else
    missing=()
    if ! command -v cc >/dev/null || ! command -v make >/dev/null; then
        if [[ $manager == apt-get ]]; then
            missing+=(build-essential)
        else
            missing+=(gcc gcc-c++ make)
        fi
    fi
    for tool in pkg-config git curl; do
        command -v "$tool" >/dev/null || missing+=("$tool")
    done
    if ((${#missing[@]})); then
        if [[ $manager == apt-get ]]; then
            privileged apt-get update
        fi
        if "$yes"; then
            privileged "$manager" install -y "${missing[@]}"
        else
            privileged "$manager" install "${missing[@]}"
        fi
    fi
fi
for tool in cc make pkg-config git curl; do
    command -v "$tool" >/dev/null || die "Build prerequisite unavailable: $tool"
done
printf 'Build prerequisites ready.\n'

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
if ! command -v rustup >/dev/null; then
    installer=$(mktemp)
    trap 'rm -f -- "$installer"' EXIT
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
    rustup_options=(--profile minimal --default-toolchain none)
    "$yes" && rustup_options+=(-y)
    sh "$installer" "${rustup_options[@]}"
    rm -f -- "$installer"
    trap - EXIT
fi
toolchain=$(sed -nE 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' rust-toolchain.toml)
[[ -n $toolchain && $toolchain != *$'\n'* ]] || die 'Expected one channel in rust-toolchain.toml.'
rustup toolchain install "$toolchain" --profile minimal --component clippy --component rustfmt
cargo_version=$(rustup run "$toolchain" cargo --version)
[[ $cargo_version == "cargo $toolchain "* ]] || die "Pinned cargo version mismatch: expected $toolchain."
printf 'Rust toolchain ready: %s (clippy, rustfmt).\n' "$toolchain"
if "$runtime_only"; then
    printf 'Skipping development tools (--runtime-only).\n'
else
    for tool in nextest insta; do
        if rustup run "$toolchain" cargo "$tool" --version >/dev/null 2>&1; then
            printf 'cargo-%s already installed.\n' "$tool"
        else
            rustup run "$toolchain" cargo install --locked "cargo-$tool"
        fi
    done
fi

# noclobber also protects dangling symlinks and concurrent setup runs.
if [[ -e .env || -L .env ]]; then
    printf 'Preserving existing .env.\n'
    # find's permission predicate works with both BSD and GNU find.
    if [[ -n $(find -L .env -prune \( -perm -0040 -o -perm -0004 \) -print) ]]; then
        printf 'Warning: existing .env is readable by group or others; consider chmod 600 .env.\n' >&2
    fi
elif (umask 077; set -o noclobber; cat .env.example > .env); then
    chmod 600 .env
    printf 'Created .env with mode 0600.\n'
else
    die 'Could not create .env; existing configuration has been preserved.'
fi

# Inspect only presence; never execute dotenv contents or emit their values.
key_present=false
if [[ ${OPENROUTER_API_KEY+x} ]]; then
    [[ -z $OPENROUTER_API_KEY ]] || key_present=true
elif awk '
    /^[[:space:]]*(export[[:space:]]+)?OPENROUTER_API_KEY[[:space:]]*=/ {
        sub(/^[^=]*=[[:space:]]*/, "")
        sub(/[[:space:]]+#.*$/, "")
        sub(/[[:space:]]*$/, "")
        present = ($0 != "" && $0 != "\"\"" && $0 != "\047\047" && $0 !~ /^#/)
    }
    END { exit !present }
' .env; then
    key_present=true
fi
if "$key_present"; then
    printf 'OPENROUTER_API_KEY is set.\n'
else
    printf 'OPENROUTER_API_KEY is required. Add it to .env or the process environment.\n'
fi

status=0
if "$no_build"; then
    printf 'Skipping release build and built-binary diagnostics (--no-build).\n'
else
    rustup run "$toolchain" cargo build --release --locked
    binary="${CARGO_TARGET_DIR:-target}/release/squad"
    [[ -x $binary ]] || die "Built binary not found: $binary"
    help=$("$binary" --help)
    if [[ $help =~ (^|[[:space:]])doctor([[:space:]]|$) ]]; then
        if "$binary" doctor; then
            printf 'squad doctor succeeded (exit 0).\n'
        else
            status=$?
            printf 'squad doctor failed (exit %s).\n' "$status" >&2
        fi
    else
        printf 'Skipping doctor: this binary does not support it yet.\n'
    fi
fi
printf 'Next command: ./target/release/squad\n'
exit "$status"

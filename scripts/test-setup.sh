#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"
"$BASH" ./setup.sh --help >/dev/null
# Keep forbidden banner programs out of both installation and invocation paths.
if grep -Ei "(^|[^[:alnum:]_])(toilet|figlet)([^[:alnum:]_]|$)" setup.sh; then
    printf "Forbidden banner program in setup.sh.\n" >&2
    exit 1
fi
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
cp setup.sh rust-toolchain.toml .env.example "$fixture/"
mkdir "$fixture/bin" "$fixture/cargo-home"
# A closed PATH prevents installed host tools from masking missing prerequisites.
for tool in bash sh env cat chmod dirname mktemp rm sed awk find mkdir ln; do
    ln -s "$(command -v "$tool")" "$fixture/bin/$tool"
done
cat > "$fixture/bin/mock" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
name=${0##*/}
case "$name" in
    uname)
        case "$1" in
            -s) printf "%s\n" "${TEST_OS:-Linux}" ;;
            -m) printf "%s\n" "${TEST_ARCH:-x86_64}" ;;
        esac ;;
    id) printf "%s\n" "${TEST_UID:-1000}" ;;
    sudo)
        printf "EXEC sudo %s\n" "$*"
        [[ $1 != -n ]] || shift
        "$@" ;;
    apt-get|dnf)
        printf "EXEC %s %s\n" "$name" "$*"
        if [[ $1 == install ]]; then
            ln -sf mock "$TEST_BIN/pkg-config"
        fi ;;
    curl)
        printf "curl %s\n" "$*" >> "$TEST_LOG"
        while [[ $1 != -o ]]; do shift; done
        cat > "$2" <<'INSTALL'
set -eu
printf "rustup-init %s\n" "$*" >> "$TEST_LOG"
mkdir -p "$CARGO_HOME/bin"
ln -s "$TEST_BIN/mock" "$CARGO_HOME/bin/rustup"
INSTALL
        ;;
    rustup)
        printf "rustup %s\n" "$*" >> "$TEST_LOG"
        if [[ $1 == run ]]; then
            [[ $2 == "$TEST_PIN" && $3 == cargo ]]
            shift 3
            "$TEST_BIN/pinned-cargo" "$@"
        fi ;;
    cargo)
        # Homebrew cargo neither accepts +toolchain nor honors the repo pin.
        printf "HOST CARGO WAS USED\n" >&2
        exit 101 ;;
    pinned-cargo)
        printf "pinned cargo %s\n" "$*" >> "$TEST_LOG"
        case "$1" in
            --version) printf "cargo %s (fixture)\n" "${TEST_CARGO_VERSION:-$TEST_PIN}" ;;
            nextest|insta) [[ ${TEST_DEV_MISSING:-false} != true ]] ;;
            install) : ;;
            build)
                mkdir -p target/release
                ln -sf "$TEST_BIN/mock" target/release/squad ;;
        esac ;;
    squad)
        if [[ $1 == --help ]]; then
            printf "Commands: doctor\n"
        elif [[ -n ${OPENROUTER_API_KEY:-} ]]; then
            printf "Doctor PASS\n"
        else
            printf "OPENROUTER_API_KEY is required\n" >&2
            exit 1
        fi ;;
    cc|make|pkg-config|git|brew|xcode-select) : ;;
    *) exit 1 ;;
esac
MOCK
chmod +x "$fixture/bin/mock"
for tool in uname id sudo cargo pinned-cargo rustup cc make pkg-config git curl brew xcode-select apt-get; do
    ln -s mock "$fixture/bin/$tool"
done
# Mock Cargo writes only the fixture's target/release directory. A runner's
# persistent build-cache override must not redirect the installer's lookup.
unset CARGO_TARGET_DIR
export CARGO_HOME="$fixture/cargo-home"
export TEST_BIN="$fixture/bin" TEST_LOG="$fixture/commands.log"
TEST_PIN=$(sed -nE 's/^channel = "([^"]+)"/\1/p' rust-toolchain.toml)
export TEST_PIN
unset OPENROUTER_API_KEY
cd -- "$fixture"
run_setup() {
    : > "$TEST_LOG"
    PATH="$TEST_BIN" "$BASH" ./setup.sh "$@" > output 2>&1 < /dev/null
}
reject_setup() {
    if run_setup "$@"; then
        printf "Setup unexpectedly succeeded: %s\n" "$*" >&2
        exit 1
    fi
}
# Preservation, permission warning, and the no-build/runtime-only switches.
printf "KEEP_THIS_CONFIGURATION=yes\n" > .env
chmod 640 .env
cp .env expected-env
run_setup --yes --runtime-only --no-build
cmp -s .env expected-env
[[ -n $(find .env -perm 0640 -print) ]]
grep -q "Preserving existing .env" output
grep -q "Warning: existing .env is readable" output
grep -q "OPENROUTER_API_KEY is required" output
grep -q "Skipping release build" output
if grep -Eq "cargo (build|install)" "$TEST_LOG"; then exit 1; fi
chmod 604 .env
run_setup --yes --no-build
grep -q "Warning: existing .env is readable" output
chmod 600 .env
run_setup --yes --no-build
if grep -q "Warning: existing .env" output; then exit 1; fi
rm .env
run_setup --yes --no-build
cmp -s .env .env.example
[[ -n $(find .env -perm 0600 -print) ]]
grep -q "Created .env with mode 0600" output
# Existing secrets stay private, including when the caller enables tracing.
printf "OPENROUTER_API_KEY=sk-fixture\n" > .env
PATH="$TEST_BIN" "$BASH" -x ./setup.sh --yes --no-build > output 2>&1 < /dev/null
grep -q "OPENROUTER_API_KEY is set" output
if grep -q "sk-fixture" output; then exit 1; fi
# Homebrew rust and rustup coexist: every cargo call must use rustup run.
TEST_OS=Darwin TEST_ARCH=arm64 TEST_DEV_MISSING=true OPENROUTER_API_KEY=sk-fixture run_setup --yes
grep -q "Platform: Darwin/arm64; package manager: brew" output
grep -q "rustup run $TEST_PIN cargo --version" "$TEST_LOG"
grep -q "pinned cargo install --locked cargo-nextest" "$TEST_LOG"
grep -q "pinned cargo install --locked cargo-insta" "$TEST_LOG"
grep -q "pinned cargo build --release --locked" "$TEST_LOG"
grep -q "squad doctor succeeded" output
TEST_CARGO_VERSION=0.0.0 reject_setup --yes --no-build
grep -q "Pinned cargo version mismatch" output
# Noninteractive sudo refuses before executing; --yes prints before execution.
rm "$TEST_BIN/pkg-config"
reject_setup --no-build
grep -q "Unattended sudo requires --yes" output
if grep -q "EXEC" output; then exit 1; fi
run_setup --yes --no-build
awk '/Running: sudo -n apt-get update/ {printed=1} /EXEC sudo -n apt-get update/ {if (!printed) exit 1; ran=1} END {if (!ran) exit 1}' output
awk '/Running: sudo -n apt-get install -y pkg-config/ {printed=1} /EXEC sudo -n apt-get install -y pkg-config/ {if (!printed) exit 1; ran=1} END {if (!ran) exit 1}' output
# Root gets the same consent rule, printed commands and noninteractive flags.
rm "$TEST_BIN/pkg-config"
TEST_UID=0 reject_setup --no-build
grep -q "Unattended installation as root requires --yes" output
if grep -q "EXEC" output; then exit 1; fi
TEST_UID=0 run_setup --yes --no-build
awk '/Running: apt-get update/ {printed=1} /EXEC apt-get update/ {if (!printed) exit 1; ran=1} END {if (!ran) exit 1}' output
grep -q "Running: apt-get install -y pkg-config" output
awk '/Running: apt-get install -y pkg-config/ {printed=1} /EXEC apt-get install -y pkg-config/ {if (!printed) exit 1; ran=1} END {if (!ran) exit 1}' output
# Fedora follows the dnf path with passwordless sudo and -y too.
rm "$TEST_BIN/apt-get" "$TEST_BIN/pkg-config"
ln -s mock "$TEST_BIN/dnf"
run_setup --yes --no-build
grep -q "Running: sudo -n dnf install -y pkg-config" output
grep -q "EXEC dnf install -y pkg-config" output
# Downloaded rustup installs into CARGO_HOME and is used immediately.
rm "$TEST_BIN/rustup"
run_setup --yes --no-build
grep -q "curl --proto =https --tlsv1.2 -fsSL https://sh.rustup.rs -o" "$TEST_LOG"
grep -q "rustup-init --profile minimal --default-toolchain none -y" "$TEST_LOG"
grep -q "rustup toolchain install $TEST_PIN" "$TEST_LOG"
# Full setup succeeds with a dummy process key and propagates doctor failure.
OPENROUTER_API_KEY=sk-fixture run_setup --yes
grep -q "squad doctor succeeded (exit 0)" output
if grep -q "sk-fixture" output; then exit 1; fi
OPENROUTER_API_KEY='' reject_setup --yes
grep -q "OPENROUTER_API_KEY is required" output
grep -q "squad doctor failed (exit 1)" output
TEST_ARCH=aarch64 reject_setup --yes --no-build
grep -q "Unsupported platform" output
reject_setup --invalid
grep -q "Unknown option" output
printf "setup.sh checks passed.\n"

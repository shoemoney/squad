#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"
./setup.sh --help >/dev/null

fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
cp setup.sh rust-toolchain.toml .env.example "$fixture/"
mkdir "$fixture/bin"
# All installation commands are stubbed: no downloads, sudo, or host changes.
cat > "$fixture/bin/mock" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
case "${0##*/}" in
    uname)
        case "$1" in
            -s) printf '%s\n' "${TEST_OS:-Linux}" ;;
            -m) printf '%s\n' "${TEST_ARCH:-x86_64}" ;;
        esac ;;
    cargo|rustup) printf '%s %s\n' "${0##*/}" "$*" >> "$TEST_LOG" ;;
    cc|make|pkg-config|git|curl|brew|xcode-select|apt-get) exit 0 ;;
    *) exit 1 ;;
esac
MOCK
chmod +x "$fixture/bin/mock"
for tool in uname cargo rustup cc make pkg-config git curl brew xcode-select apt-get; do
    ln -s mock "$fixture/bin/$tool"
done
export PATH="$fixture/bin:$PATH"
export CARGO_HOME="$fixture/cargo-home"
export TEST_LOG="$fixture/commands.log"
unset OPENROUTER_API_KEY
cd -- "$fixture"
printf 'KEEP_THIS_CONFIGURATION=yes\n' > .env
chmod 640 .env
cp .env expected-env
./setup.sh --yes --runtime-only --no-build > output
cmp -s .env expected-env
[[ -n $(find .env -perm 0640 -print) ]]
grep -q 'Preserving existing .env' output
grep -q 'OPENROUTER_API_KEY is required' output
grep -q 'Skipping release build and built-binary diagnostics' output
if grep -Eq 'build|install --locked' "$TEST_LOG"; then
    printf 'Skipped commands unexpectedly ran.\n' >&2
    exit 1
fi

rm .env
./setup.sh --yes --no-build > output
cmp -s .env .env.example
[[ -n $(find .env -perm 0600 -print) ]]
grep -q 'Created .env with mode 0600' output
TEST_OS=Darwin TEST_ARCH=arm64 ./setup.sh --yes --no-build > output
grep -q 'Platform: Darwin/arm64; package manager: brew' output
if TEST_ARCH=aarch64 ./setup.sh --yes --no-build > output 2>&1; then
    printf 'Unsupported platform unexpectedly succeeded.\n' >&2
    exit 1
fi
grep -q 'Unsupported platform' output
if ./setup.sh --invalid > output 2>&1; then
    printf 'Unknown option unexpectedly succeeded.\n' >&2
    exit 1
fi
printf 'setup.sh checks passed.\n'

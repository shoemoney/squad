#!/usr/bin/env bash
# Run as root on an ARM Mac; registration token arrives on stdin, never argv.
set -euo pipefail
set +x
umask 077
if [[ ${1:-} == --help ]]; then
    echo 'Usage: registration-token-command | sudo ./scripts/install-ci-macos.sh ARCHIVE SHA256'
    echo 'Installs one dedicated Squad CI account and a launchd service on an ARM Mac.'
    exit 0
fi
[[ $(uname -s)/$(uname -m) == Darwin/arm64 && $(id -u) == 0 ]] || {
    echo 'Run as root on a macOS ARM64 worker.' >&2; exit 2;
}
archive=${1:?Pass the official macOS ARM64 runner archive.}
checksum=${2:?Pass its GitHub-published SHA256 checksum.}
printf '%s  %s\n' "$checksum" "$archive" | shasum -a 256 -c -
IFS= read -r runner_token || { echo 'Missing registration token on stdin.' >&2; exit 2; }
[[ -n $runner_token ]] || { echo 'Missing registration token on stdin.' >&2; exit 2; }
ci_user=squad-ci
ci_home=/Users/squad-ci
runner=$ci_home/runner
service=ai.shoemoney.squad-ci
plist=/Library/LaunchDaemons/$service.plist
[[ ! -e $runner/.runner && ! -e $plist ]] || {
    echo 'Squad CI is already configured; inspect its service before replacing it.' >&2; exit 1;
}
if ! dscl . -read /Users/$ci_user >/dev/null 2>&1; then
    ci_uid=601
    while dscl . -list /Users UniqueID | awk '{print $2}' | grep -qx "$ci_uid"; do
        ci_uid=$((ci_uid + 1))
    done
    dscl . -create /Users/$ci_user
    dscl . -create /Users/$ci_user UniqueID "$ci_uid"
    dscl . -create /Users/$ci_user PrimaryGroupID 20
    dscl . -create /Users/$ci_user NFSHomeDirectory "$ci_home"
    dscl . -create /Users/$ci_user UserShell /usr/bin/false
    dscl . -create /Users/$ci_user RealName 'Squad CI'
    dscl . -create /Users/$ci_user Password '*'
    dscl . -create /Users/$ci_user IsHidden 1
fi
[[ $(dscl . -read /Users/$ci_user NFSHomeDirectory | awk '{print $2}') == "$ci_home" ]] || {
    echo 'Existing CI user has an unexpected home directory.' >&2; exit 1;
}
mkdir -p "$runner" "$ci_home/logs" "$ci_home/target"
tar -xzf "$archive" -C "$runner"
chown -R "$ci_user":staff "$ci_home"
chmod 700 "$ci_home"
printf '%s\n' "$runner_token" | sudo -H -u "$ci_user" /bin/bash -c '
    set -euo pipefail
    set +x
    IFS= read -r ACTIONS_RUNNER_INPUT_TOKEN
    export ACTIONS_RUNNER_INPUT_TOKEN
    cd /Users/squad-ci/runner
    ./config.sh --unattended --url https://github.com/shoemoney/squad \
        --name squad-ci-macos --labels squad-ci --work _work
'
unset runner_token
cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$service</string>
<key>UserName</key><string>$ci_user</string>
<key>WorkingDirectory</key><string>$runner</string>
<key>ProgramArguments</key><array><string>$runner/run.sh</string></array>
<key>EnvironmentVariables</key><dict>
<key>HOME</key><string>$ci_home</string>
<key>PATH</key><string>$ci_home/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
<key>CARGO_BUILD_JOBS</key><string>4</string>
<key>CARGO_TARGET_DIR</key><string>$ci_home/target/squad</string>
</dict>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><true/>
<key>Nice</key><integer>10</integer>
<key>StandardOutPath</key><string>$ci_home/logs/runner.log</string>
<key>StandardErrorPath</key><string>$ci_home/logs/runner.err</string>
</dict></plist>
PLIST
chown root:wheel "$plist"
chmod 644 "$plist"
plutil -lint "$plist"
launchctl bootstrap system "$plist"

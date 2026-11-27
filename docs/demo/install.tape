# Squad install walkthrough (README). Record with: vhs docs/demo/install.tape
# Downloads the PUBLISHED release archive for Linux x64 into a throwaway HOME, verifies the
# checksum, installs the binary, then runs `squad --version` and `squad doctor`.
# The key shown is a placeholder, not a credential. Nothing here touches the network after the download.

Output docs/media/install.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 760
Set Padding 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 15ms
Set WindowBar Colorful

Hide
Type 'export HOME="$(mktemp -d -p /dev/shm)" && cd "$HOME" && export PS1="\$ " TERM=xterm-256color && clear'
Enter
Sleep 800ms
Show

Type "B=https://github.com/shoemoney/squad/releases/latest/download"
Enter
Sleep 400ms
Type "A=squad-cli-x86_64-unknown-linux-gnu"
Enter
Sleep 400ms
Type "curl -fsSL -O $B/$A.tar.xz -O $B/$A.tar.xz.sha256"
Enter
Sleep 2s
Type "sha256sum -c $A.tar.xz.sha256"
Enter
Sleep 2s
Type "tar -xJf $A.tar.xz && mkdir -p ~/.local/bin && install -m 0755 $A/squad ~/.local/bin/squad"
Enter
Sleep 2s
Type 'export PATH="$HOME/.local/bin:$PATH"'
Enter
Sleep 500ms
Type "squad --version"
Enter
Sleep 2s
Type "OPENROUTER_API_KEY=your-key-here squad doctor | grep -E '^(Config|Cred|Data dir|Overall)' | cut -c1-100"
Enter
Sleep 2s
Screenshot docs/media/doctor-pass.png
Sleep 2s

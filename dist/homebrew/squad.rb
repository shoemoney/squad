# Release template: dist/homebrew/render.py fills every @...@ placeholder.
# Rendered as an artifact only; publishing a tap requires the owner's decision.
class Squad < Formula
  desc "Terminal chat room for multiple language models through OpenRouter"
  homepage "https://github.com/shoemoney/squad"
  version "@VERSION@"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/shoemoney/squad/releases/download/@TAG@/squad-cli-aarch64-apple-darwin.tar.xz"
      sha256 "@SHA256_MACOS_ARM64@"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/shoemoney/squad/releases/download/@TAG@/squad-cli-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "@SHA256_LINUX_X64@"
    end
  end

  def install
    bin.install "squad"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/squad --version")
    assert_match "doctor", shell_output("#{bin}/squad --help")
  end
end

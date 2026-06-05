class Ccs < Formula
  desc "Multi-line, mode-switchable status line for Claude Code"
  homepage "https://github.com/hankeGui/cc-status"
  license "MIT"
  version "0.1.0"

  on_macos do
    on_arm do
      url "https://github.com/hankeGui/cc-status/releases/download/v#{version}/ccs-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_ON_RELEASE_DARWIN_ARM64"
    end
    on_intel do
      url "https://github.com/hankeGui/cc-status/releases/download/v#{version}/ccs-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_ON_RELEASE_DARWIN_X64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/hankeGui/cc-status/releases/download/v#{version}/ccs-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ON_RELEASE_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/hankeGui/cc-status/releases/download/v#{version}/ccs-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ON_RELEASE_LINUX_X64"
    end
  end

  def install
    bin.install "ccs"
  end

  test do
    assert_match "ccs", shell_output("#{bin}/ccs --version")
  end
end

class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://github.com/muhvarriel/macmop-cli"
  url "https://github.com/muhvarriel/macmop-cli/releases/download/v0.1.0-alpha.27/macmop-v0.1.0-alpha.27-source.tar.gz"
  sha256 "d5e7af1f6e8efcea27a6b4af3ee0069ec70a46a3bf833827e944947151e2e43e"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.1.0-alpha.27", shell_output("#{bin}/macmop --version")
  end
end

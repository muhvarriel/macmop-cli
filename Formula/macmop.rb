class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://github.com/muhvarriel/macmop-cli"
  url "https://github.com/muhvarriel/macmop-cli/releases/download/v0.2.0-beta.2/macmop-v0.2.0-beta.2-source.tar.gz"
  sha256 "2aba982038025a11bab45e74ce64931a897e0bfad900e4f0f91287265c0e116a"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.2.0-beta.2", shell_output("#{bin}/macmop --version")
  end
end

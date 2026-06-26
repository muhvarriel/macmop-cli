class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://github.com/muhvarriel/macmop-cli"
  url "https://github.com/muhvarriel/macmop-cli/archive/refs/tags/v0.2.0-beta.1.tar.gz"
  sha256 "bb37f6cf637cf6028f507c7bc3e437222f2ac361b2073897e2681093e2d5a613"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.2.0-beta.1", shell_output("#{bin}/macmop --version")
  end
end

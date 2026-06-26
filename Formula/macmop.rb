class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://github.com/muhvarriel/macmop-cli"
  url "https://github.com/muhvarriel/macmop-cli/releases/download/v0.2.0-beta.1/macmop-v0.2.0-beta.1-source.tar.gz"
  sha256 "e558be33563878b039070536dbdfb2d4ebaa51da31fed4c870c3e5042d1fd0aa"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.2.0-beta.1", shell_output("#{bin}/macmop --version")
  end
end

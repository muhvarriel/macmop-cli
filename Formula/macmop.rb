class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://example.com/macmop" # TODO: replace before publishing
  url "https://example.com/macmop-v0.1.0-alpha.9.tar.gz" # TODO: replace before publishing
  sha256 "TODO_REPLACE_WITH_RELEASE_TARBALL_SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.1.0-alpha.9", shell_output("#{bin}/macmop --version")
  end
end

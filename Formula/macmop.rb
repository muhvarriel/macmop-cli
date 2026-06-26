class Macmop < Formula
  desc "Safety-first macOS cleanup CLI"
  homepage "https://github.com/muhvarriel/macmop-cli"
  url "https://github.com/muhvarriel/macmop-cli/archive/refs/tags/v0.1.0-alpha.11.tar.gz" # TODO: replace before publishing
  # Draft only: replace url and sha256 after publishing a tagged release archive.
  sha256 "TODO_REPLACE_WITH_RELEASE_TARBALL_SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "macmop 0.1.0-alpha.11", shell_output("#{bin}/macmop --version")
  end
end

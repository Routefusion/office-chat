class OfficeChat < Formula
  desc "Encrypted LAN chat over UDP broadcast"
  homepage "https://github.com/routefusion/office-chat"
  # Update URL and sha256 when cutting a release:
  #   1. git tag v0.1.0 && git push --tags
  #   2. Create a GitHub release with the source tarball
  #   3. shasum -a 256 office-chat-0.1.0.tar.gz
  url "https://github.com/routefusion/office-chat/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_ACTUAL_SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "office-chat", shell_output("#{bin}/office-chat --help")
  end
end

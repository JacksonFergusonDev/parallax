"""Write the Homebrew formula for a released parallax-cli.

The release workflow runs this with the version and the two CLI archives,
then commits the result to JacksonFergusonDev/homebrew-tap as
Formula/parallax-cli.rb. The formula installs the prebuilt binary and the
kepubify beside it, so Homebrew users compile nothing.
"""

import argparse
import hashlib
import sys
from pathlib import Path

REPOSITORY = "https://github.com/JacksonFergusonDev/parallax"
TARGETS = {"arm": "aarch64-apple-darwin", "intel": "x86_64-apple-darwin"}

TEMPLATE = """\
# Written by parallax's release workflow (scripts/homebrew_formula.py).
# Edit the template there, not this file.
class ParallaxCli < Formula
  desc "Manage your Parallax library and Kobo from the terminal"
  homepage "https://parallax.jacksonferguson.me"
  version "{version}"
  license "GPL-3.0-only"

  depends_on :macos

  on_arm do
    url "{arm_url}"
    sha256 "{arm_sha256}"
  end

  on_intel do
    url "{intel_url}"
    sha256 "{intel_sha256}"
  end

  def install
    libexec.install "parallax-cli", "kepubify"
    (bin/"parallax-cli").write_env_script libexec/"parallax-cli", PARALLAX_KEPUBIFY: libexec/"kepubify"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/parallax-cli --version")
  end
end
"""


def archive_name(version: str, target: str) -> str:
    return f"parallax-cli-{version}-{target}.tar.gz"


def archive_url(version: str, target: str) -> str:
    return f"{REPOSITORY}/releases/download/v{version}/{archive_name(version, target)}"


def render(version: str, sha256: dict[str, str]) -> str:
    """The formula for `version`, given each architecture's archive checksum."""
    return TEMPLATE.format(
        version=version,
        arm_url=archive_url(version, TARGETS["arm"]),
        arm_sha256=sha256["arm"],
        intel_url=archive_url(version, TARGETS["intel"]),
        intel_sha256=sha256["intel"],
    )


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("version")
    parser.add_argument("archives", type=Path, help="folder holding both CLI archives")
    parser.add_argument("output", type=Path, help="where to write the formula")
    args = parser.parse_args(argv)
    try:
        sha256 = {
            arch: digest(args.archives / archive_name(args.version, target))
            for arch, target in TARGETS.items()
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(render(args.version, sha256))
    except OSError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""Write the Homebrew formula for a released parallax-cli.

The release workflow runs this with the version and the two CLI archives,
then commits the result to JacksonFergusonDev/homebrew-tap as
Formula/parallax-cli.rb. The formula installs the prebuilt binary and the
kepubify beside it, so Homebrew users compile nothing. `--head OUTPUT`
writes the version without a release, which only builds main from source;
the tap carries that one until the first release replaces it.
"""

import argparse
import hashlib
import sys
from pathlib import Path

REPOSITORY = "https://github.com/JacksonFergusonDev/parallax"
TARGETS = {"arm": "aarch64-apple-darwin", "intel": "x86_64-apple-darwin"}

HEADER = """\
# Written by parallax's release workflow (scripts/homebrew_formula.py).
# Edit the template there, not this file.
class ParallaxCli < Formula
  desc "Manage your Parallax library and Kobo from the terminal"
  homepage "https://parallax.jacksonferguson.me"
"""

# Homebrew's style rules don't allow `url` inside `on_arm` or `on_intel`.
STABLE = """\
  if Hardware::CPU.arm?
    url "{arm_url}"
    sha256 "{arm_sha256}"
  else
    url "{intel_url}"
    sha256 "{intel_sha256}"
  end
  version "{version}"
"""

LICENSE = """\
  license "GPL-3.0-only"
"""

# `brew install --HEAD parallax-cli` builds main from source against
# Homebrew's kepubify; a stable install takes the release's prebuilt pair.
BODY = """\

  head do
    url "https://github.com/JacksonFergusonDev/parallax.git", branch: "main"

    depends_on "rust" => :build
    depends_on "kepubify"
  end

  depends_on :macos

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args(root: libexec, path: "crates/cli")
      kepubify = formula_opt_bin("kepubify")/"kepubify"
      (bin/"parallax-cli").write_env_script libexec/"bin/parallax-cli", PARALLAX_KEPUBIFY: kepubify
    else
      libexec.install "parallax-cli", "kepubify"
      (bin/"parallax-cli").write_env_script libexec/"parallax-cli", PARALLAX_KEPUBIFY: libexec/"kepubify"
    end
  end

  test do
    assert_match(/^parallax-cli \\d+\\.\\d+\\.\\d+/, shell_output("#{bin}/parallax-cli --version"))
  end
end
"""


def archive_name(version: str, target: str) -> str:
    return f"parallax-cli-{version}-{target}.tar.gz"


def archive_url(version: str, target: str) -> str:
    return f"{REPOSITORY}/releases/download/v{version}/{archive_name(version, target)}"


def render(version: str | None, sha256: dict[str, str] | None = None) -> str:
    """The formula for `version`, given each architecture's archive checksum,
    or with no version, one that only builds main (`--HEAD`), for before the
    first release."""
    if version is None or sha256 is None:
        return HEADER + LICENSE + BODY
    stable = STABLE.format(
        version=version,
        arm_url=archive_url(version, TARGETS["arm"]),
        arm_sha256=sha256["arm"],
        intel_url=archive_url(version, TARGETS["intel"]),
        intel_sha256=sha256["intel"],
    )
    return HEADER + stable + LICENSE + BODY


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("version", nargs="?")
    parser.add_argument(
        "archives", nargs="?", type=Path, help="folder holding both CLI archives"
    )
    parser.add_argument("output", type=Path, help="where to write the formula")
    parser.add_argument(
        "--head",
        action="store_true",
        help="write the formula that only builds main, for before the first release",
    )
    args = parser.parse_args(argv)
    if args.head == (args.version is not None):
        parser.error("give either --head OUTPUT or VERSION ARCHIVES OUTPUT")
    try:
        if args.head:
            formula = render(None)
        else:
            formula = render(
                args.version,
                {
                    arch: digest(args.archives / archive_name(args.version, target))
                    for arch, target in TARGETS.items()
                },
            )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(formula)
    except OSError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

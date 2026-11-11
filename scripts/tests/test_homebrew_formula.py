import hashlib
from pathlib import Path

import homebrew_formula


def test_render_points_each_architecture_at_its_archive() -> None:
    formula = homebrew_formula.render("0.1.0", {"arm": "a" * 64, "intel": "b" * 64})

    assert 'version "0.1.0"' in formula
    assert (
        "if Hardware::CPU.arm?\n"
        '    url "https://github.com/JacksonFergusonDev/parallax/releases/download/v0.1.0/'
        'parallax-cli-0.1.0-aarch64-apple-darwin.tar.gz"\n    sha256 "' + "a" * 64
    ) in formula
    assert (
        'parallax-cli-0.1.0-x86_64-apple-darwin.tar.gz"\n    sha256 "' + "b" * 64
        in formula
    )
    # Ruby's interpolation survives Python's formatting.
    assert 'shell_output("#{bin}/parallax-cli --version")' in formula
    assert (
        'head do\n    url "https://github.com/JacksonFergusonDev/parallax.git"'
        in formula
    )


def test_head_only_builds_main_and_names_no_release(tmp_path: Path) -> None:
    output = tmp_path / "Formula/parallax-cli.rb"
    assert homebrew_formula.main(["--head", str(output)]) == 0

    formula = output.read_text()
    assert "head do" in formula
    assert "version " not in formula
    assert "Hardware::CPU" not in formula
    assert "releases/download" not in formula


def test_main_hashes_the_archives(tmp_path: Path) -> None:
    for target, content in [
        ("aarch64-apple-darwin", b"arm"),
        ("x86_64-apple-darwin", b"x64"),
    ]:
        (tmp_path / f"parallax-cli-1.0.0-{target}.tar.gz").write_bytes(content)
    output = tmp_path / "parallax-cli.rb"

    assert homebrew_formula.main(["1.0.0", str(tmp_path), str(output)]) == 0

    text = output.read_text()
    assert hashlib.sha256(b"arm").hexdigest() in text
    assert hashlib.sha256(b"x64").hexdigest() in text


def test_main_reports_a_missing_archive(tmp_path: Path) -> None:
    assert homebrew_formula.main(["1.0.0", str(tmp_path), str(tmp_path / "f.rb")]) == 1

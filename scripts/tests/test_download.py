"""Check that failed downloads leave installed artifacts untouched."""

import hashlib
import time
from io import BytesIO
from pathlib import Path
from urllib.error import URLError

import pytest

import download

URL = "https://example.com/sidecar"
DATA = b"expected sidecar"
SHA256 = hashlib.sha256(DATA).hexdigest()


def test_install_retries_then_reuses_verified_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    requests = 0

    def open_url(url: str, *, timeout: int) -> BytesIO:
        nonlocal requests
        requests += 1
        if requests == 1:
            raise URLError("temporary connection failure")
        return BytesIO(DATA)

    monkeypatch.setattr(download, "urlopen", open_url)
    monkeypatch.setattr(time, "sleep", lambda seconds: None)
    destination = tmp_path / "bin" / "sidecar"
    assert download.download_verified(URL, destination, SHA256, executable=True)
    assert destination.read_bytes() == DATA
    assert not download.download_verified(URL, destination, SHA256, executable=True)
    assert requests == 2
    assert list(destination.parent.iterdir()) == [destination]


def test_checksum_failure_preserves_existing_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        download, "urlopen", lambda *args, **kwargs: BytesIO(b"corrupt download")
    )
    destination = tmp_path / "sidecar"
    destination.write_bytes(b"installed sidecar")
    with pytest.raises(ValueError, match="checksum mismatch for sidecar"):
        download.download_verified(URL, destination, SHA256)
    assert destination.read_bytes() == b"installed sidecar"
    assert list(tmp_path.iterdir()) == [destination]

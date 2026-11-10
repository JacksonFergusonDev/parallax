"""Download pinned build inputs and publish only verified bytes."""

import hashlib
import shutil
import time
from pathlib import Path
from tempfile import NamedTemporaryFile
from urllib.error import HTTPError
from urllib.request import urlopen

RETRYABLE_HTTP = {408, 429, 500, 502, 503, 504}


def matches(path: Path, sha256: str) -> bool:
    try:
        with path.open("rb") as source:
            return hashlib.file_digest(source, "sha256").hexdigest() == sha256
    except FileNotFoundError:
        return False


def download_verified(
    url: str, destination: Path, sha256: str, *, executable: bool = False
) -> bool:
    """Return whether the destination needed a download."""
    if matches(destination, sha256):
        if executable:
            destination.chmod(destination.stat().st_mode | 0o111)
        return False

    destination.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(
        dir=destination.parent, prefix=f"{destination.name}.", delete=False
    ) as temporary:
        part = Path(temporary.name)

    try:
        for attempt in range(4):
            try:
                with urlopen(url, timeout=30) as response, part.open("wb") as output:
                    shutil.copyfileobj(response, output)
                break
            except OSError as error:
                if attempt == 3 or (
                    isinstance(error, HTTPError) and error.code not in RETRYABLE_HTTP
                ):
                    raise
                time.sleep(2**attempt)

        if not matches(part, sha256):
            raise ValueError(f"checksum mismatch for {url.rsplit('/', 1)[-1]}")
        if executable:
            part.chmod(0o755)
        part.replace(destination)
    finally:
        part.unlink(missing_ok=True)
    return True

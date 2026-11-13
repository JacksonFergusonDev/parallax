#!/usr/bin/env python3
"""Anonymizes a Goodreads library export (CSV) into a fixture safe to commit.

Usage: make_goodreads_fixture.py EXPORT.csv OUTPUT.csv [--seed N]

The output keeps the export's shape, so parsers see what Goodreads really sends:
the same columns, row order, quoting, `="..."` ISBN wrappers, `(Series, #N)`
titles, `<br/>` in reviews, curly and straight apostrophes, non-Latin text,
shelves with positions, blank cells and date formats. It replaces everything
that identifies the books or their reader:

- Words in titles, authors, publishers and reviews become pseudo-words of the
  same length, vowel and consonant pattern, case and script. A word always maps
  to the same pseudo-word, so series names, authors and `Author l-f` stay
  consistent. Short function words (the, of, de, van) and generic publisher
  words (Press, Books) stay, so article-stripping and sorting still behave.
  No other original word of three or more letters survives in the output.
- Book Ids are random numbers of the same length. ISBN-10 and ISBN-13 are new
  valid pairs; an ISBN-10 ending in X still ends in X.
- Dates all move by one random offset, which keeps every interval and order.
- Page counts move by up to 10%.

Ratings, years, bindings, shelves, read counts and the row order are kept.
Each run draws a new random mapping unless `--seed` is given. Don't commit the
seed you used, and never commit the export itself.
"""

import argparse
import csv
import datetime
import random
import re
import sys
import unicodedata
from collections.abc import Sequence
from pathlib import Path

WORD = re.compile(r"[^\W\d_]+")
TAG = re.compile(r"(<[^>]*>)")  # reviews hold <br/>; the markup is not private
APOSTROPHES = "'\u2019\u2018`"
CONTRACTIONS = {"s", "t", "d", "ll", "re", "ve", "m"}
# Short function words keep titles and reviews natural and article-stripping sorts
# working; generic publisher and series words keep those looking like themselves.
KEPT = frozenset(
    """
    a an the of and or in on at to for from with by as is it its i be was are
    but not that this these those my me you your he she we they his her our
    if so do did has had have will would can all one no than then what who
    de del der di du la las le les van von vs el al und et da dos
    press books book publishing publishers publications group house inc llc ltd
    company co editions classics library media university vol volume series saga
    """.split()  # noqa: SIM905
)
TEXT_COLUMNS = (
    "Title",
    "Author",
    "Author l-f",
    "Additional Authors",
    "Publisher",
    "My Review",
    "Spoiler",
    "Private Notes",
)
VOWELS = "aeiou"
CONSONANTS = "bcdfghjklmnprstvwz"
ACCENTED_VOWELS = "àáâäèéêëìíîïòóôöùúûü"
ACCENTED_CONSONANTS = "çñšžčř"
SCRIPT_RANGES = {
    "KATAKANA": (0x30A1, 0x30F6),
    "HIRAGANA": (0x3041, 0x3096),
    "CJK": (0x4E00, 0x9FFF),
    "HANGUL": (0xAC00, 0xD7A3),
    "CYRILLIC": (0x0430, 0x044F),
    "GREEK": (0x03B1, 0x03C9),
}


def isbn13_check(first12: str) -> str:
    total = sum(int(d) * (1 if i % 2 == 0 else 3) for i, d in enumerate(first12))
    return str((10 - total % 10) % 10)


def isbn10_check(first9: str) -> str:
    total = sum(int(d) * (10 - i) for i, d in enumerate(first9))
    check = (11 - total % 11) % 11
    return "X" if check == 10 else str(check)


def unwrap(cell: str) -> str:
    """Goodreads writes ISBNs as ="0618057072" so spreadsheets keep leading zeros."""
    match = re.fullmatch(r'="(.*)"', cell)
    return match.group(1) if match else cell


def wrap(value: str) -> str:
    return f'="{value}"'


class Anonymizer:
    def __init__(self, rows: Sequence[dict[str, str]], seed: int | None = None):
        self.rng = random.Random(seed)
        self.words: dict[str, str] = {}
        self.original_words = {
            word.lower()
            for row in rows
            for column in TEXT_COLUMNS
            for word in WORD.findall(TAG.sub(" ", row.get(column, "")))
        }
        self.used_ids: set[str] = set()
        self.day_offset = self.rng.randint(-900, -300)

    def letter(self, char: str) -> str:
        if char.isascii():
            pool = VOWELS if char.lower() in VOWELS else CONSONANTS
        else:
            name = unicodedata.name(char, "")
            for script, (low, high) in SCRIPT_RANGES.items():
                if name.startswith(script):
                    return chr(self.rng.randint(low, high))
            pool = (
                ACCENTED_VOWELS
                if unicodedata.normalize("NFD", char)[0].lower() in VOWELS
                else ACCENTED_CONSONANTS
            )
        picked = self.rng.choice(pool)
        return picked.upper() if char.isupper() else picked

    def pseudo(self, word: str) -> str:
        if word in self.words:
            return self.words[word]
        fake = word
        for _ in range(100):
            fake = "".join(self.letter(char) for char in word)
            # Initials and other tiny words can't all avoid real words, and need not.
            if len(word) < 3 or (
                fake.lower() not in self.original_words and fake.lower() not in KEPT
            ):
                break
        self.words[word] = fake
        return fake

    def words_in(self, value: str) -> str:
        def replace(match: re.Match[str]) -> str:
            word = match.group()
            before = value[match.start() - 1] if match.start() else ""
            if word.lower() in KEPT:
                return word
            if before and before in APOSTROPHES and word.lower() in CONTRACTIONS:
                return word
            return self.pseudo(word)

        return WORD.sub(replace, value)

    def text(self, value: str) -> str:
        return "".join(
            part if TAG.fullmatch(part) else self.words_in(part)
            for part in TAG.split(value)
        )

    def book_id(self, original: str) -> str:
        while True:
            first = self.rng.randint(1, 9)
            rest = "".join(
                str(self.rng.randint(0, 9)) for _ in range(len(original) - 1)
            )
            fresh = f"{first}{rest}"[: len(original)]
            if fresh not in self.used_ids:
                self.used_ids.add(fresh)
                return fresh

    def isbns(self, isbn10: str, isbn13: str) -> tuple[str, str]:
        """New valid ISBNs where the export had them, with the same X-or-not ending."""
        old10, old13 = unwrap(isbn10), unwrap(isbn13)
        group = (old10 or old13[3:])[:1] or "0"
        prefix = old13[:3] if old13 else "978"
        for _ in range(1000):
            body = group + "".join(str(self.rng.randint(0, 9)) for _ in range(8))
            if old10 and (isbn10_check(body) == "X") != old10.endswith("X"):
                continue
            new10 = body + isbn10_check(body) if old10 else ""
            first12 = prefix + body
            new13 = first12 + isbn13_check(first12) if old13 else ""
            return wrap(new10), wrap(new13)
        raise RuntimeError("could not generate ISBNs")

    def date(self, value: str) -> str:
        if not value:
            return value
        year, month, day = (int(part) for part in value.split("/"))
        moved = datetime.date(year, month, day) + datetime.timedelta(
            days=self.day_offset
        )
        return f"{moved.year:04d}/{moved.month:02d}/{moved.day:02d}"

    def pages(self, value: str) -> str:
        if not value:
            return value
        return str(max(1, round(int(value) * self.rng.uniform(0.9, 1.1))))

    def row(self, row: dict[str, str]) -> dict[str, str]:
        out = dict(row)
        for column in TEXT_COLUMNS:
            out[column] = self.text(row[column])
        out["Book Id"] = self.book_id(row["Book Id"])
        out["ISBN"], out["ISBN13"] = self.isbns(row["ISBN"], row["ISBN13"])
        out["Date Read"] = self.date(row["Date Read"])
        out["Date Added"] = self.date(row["Date Added"])
        out["Number of Pages"] = self.pages(row["Number of Pages"])
        return out


def anonymize(source: Path, dest: Path, seed: int | None = None) -> int:
    with source.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        fields = list(reader.fieldnames or [])
        rows = list(reader)
    anonymizer = Anonymizer(rows, seed)
    with dest.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fields, lineterminator="\n")
        writer.writeheader()
        writer.writerows(anonymizer.row(row) for row in rows)
    return len(rows)


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("source", type=Path, help="the Goodreads export")
    parser.add_argument("dest", type=Path, help="where to write the fixture")
    parser.add_argument("--seed", type=int, help="make the mapping repeatable")
    args = parser.parse_args(argv)
    count = anonymize(args.source, args.dest, args.seed)
    print(f"Wrote {count} anonymized rows to {args.dest}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

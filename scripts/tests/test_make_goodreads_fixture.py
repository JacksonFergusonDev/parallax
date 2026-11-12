import csv
import datetime
import re
from pathlib import Path

import make_goodreads_fixture as fixture

FIELDS = [
    "Book Id",
    "Title",
    "Author",
    "Author l-f",
    "Additional Authors",
    "ISBN",
    "ISBN13",
    "My Rating",
    "Publisher",
    "Binding",
    "Number of Pages",
    "Year Published",
    "Original Publication Year",
    "Date Read",
    "Date Added",
    "Bookshelves",
    "Bookshelves with positions",
    "Exclusive Shelf",
    "My Review",
    "Spoiler",
    "Private Notes",
    "Read Count",
    "Owned Copies",
]


def book(**values: str) -> dict[str, str]:
    row = dict.fromkeys(FIELDS, "")
    row.update(
        {
            "Book Id": "12345678",
            "Title": "Zorblax Quintessence (Wumbo Saga, #2)",
            "Author": "Quentin Z. Frobnicate",
            "Author l-f": "Frobnicate, Quentin Z.",
            "ISBN": '="061805707X"',
            "ISBN13": '="9780618057078"',
            "Publisher": "Glorp Press",
            "Number of Pages": "400",
            "Date Read": "2026/06/21",
            "Date Added": "2026/06/16",
            "Exclusive Shelf": "read",
            "My Review": 'Gandalf\'s plot isn\u2019t "bad".<br/><br/>Cons:<br/>Zymurgy',
        }
    )
    row.update(values)
    return row


def run(tmp_path: Path, rows: list[dict[str, str]]) -> list[dict[str, str]]:
    source, dest = tmp_path / "in.csv", tmp_path / "out.csv"
    with source.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    assert fixture.anonymize(source, dest, seed=1) == len(rows)
    assert dest.read_text(encoding="utf-8").splitlines()[0] == ",".join(FIELDS)
    with dest.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def shape(value: str) -> str:
    return re.sub(r"[^\W\d_]", "a", value)


def test_keeps_shape_but_replaces_words(tmp_path: Path) -> None:
    (out,) = run(tmp_path, [book()])
    original = book()
    for column in ("Title", "Author", "Author l-f", "Publisher", "My Review"):
        assert out[column] != original[column]
        assert len(out[column]) == len(original[column])
    assert shape(out["Title"]).count("(") == 1
    assert out["Title"].endswith(", #2)")
    assert out["Publisher"].endswith(" Press")
    assert out["My Review"].count("<br/>") == 3
    for mark in ("\u2019", "'", '"'):
        assert mark in out["My Review"]
    assert re.search(r"\w+'s\b", out["My Review"])


def test_no_original_word_survives(tmp_path: Path) -> None:
    (out,) = run(tmp_path, [book()])
    text = " ".join(out[c] for c in fixture.TEXT_COLUMNS).lower()
    for word in (
        "zorblax",
        "quintessence",
        "wumbo",
        "frobnicate",
        "quentin",
        "glorp",
        "gandalf",
    ):
        assert word not in text


def test_same_word_maps_the_same_across_columns_and_rows(tmp_path: Path) -> None:
    other = book(Title="Another Frobnicate Book", **{"Book Id": "99999999"})
    first, second = run(tmp_path, [book(), other])
    last = first["Author"].split()[-1]
    assert first["Author l-f"].startswith(f"{last}, ")
    assert last in second["Title"]
    assert first["Book Id"] != second["Book Id"]


def test_non_latin_and_accented_text_stays_in_script(tmp_path: Path) -> None:
    (out,) = run(
        tmp_path,
        [book(Title="ベルセルク 41 [Berserk 41]", Author="Antoine de Saint-Exupéry")],
    )
    assert re.match(r"[ァ-ヶ]{5} 41 \[[A-Za-z]{7} 41\]", out["Title"])
    assert " de " in out["Author"]
    assert "-" in out["Author"]


def test_initials_map_the_same_in_author_and_author_lf(tmp_path: Path) -> None:
    (out,) = run(tmp_path, [book(Author="T.J. Klune", **{"Author l-f": "Klune, T.J."})])
    initials, surname = out["Author"].split()
    assert out["Author l-f"] == f"{surname}, {initials}"
    assert initials != "T.J."


def test_isbns_are_valid_and_keep_x_endings(tmp_path: Path) -> None:
    rows = run(
        tmp_path,
        [book(), book(ISBN='="0618057072"'), book(ISBN='=""', ISBN13='=""')],
    )
    assert rows[0]["ISBN"].endswith('X"')
    assert not rows[1]["ISBN"].endswith('X"')
    assert rows[2]["ISBN"] == '=""'
    assert rows[2]["ISBN13"] == '=""'
    for row in rows[:2]:
        ten, thirteen = fixture.unwrap(row["ISBN"]), fixture.unwrap(row["ISBN13"])
        assert ten[-1] == fixture.isbn10_check(ten[:-1])
        assert thirteen[-1] == fixture.isbn13_check(thirteen[:-1])
        assert thirteen[3:-1] == ten[:-1]
    assert rows[0]["ISBN"] != book()["ISBN"]


def test_dates_move_together_and_blanks_stay_blank(tmp_path: Path) -> None:
    read, unread = run(tmp_path, [book(), book(**{"Date Read": ""})])
    assert unread["Date Read"] == ""
    assert read["Date Read"] != "2026/06/21"
    assert re.fullmatch(r"\d{4}/\d{2}/\d{2}", read["Date Added"])
    finished = datetime.date(*map(int, read["Date Read"].split("/")))
    started = datetime.date(*map(int, read["Date Added"].split("/")))
    assert (finished - started).days == 5


def test_untouched_columns_and_pages(tmp_path: Path) -> None:
    shelves = {
        "My Rating": "4.0",
        "Bookshelves": "favorites, to-read",
        "Bookshelves with positions": "favorites (#2), to-read (#7)",
        "Read Count": "2",
        "Binding": "Kindle Edition",
    }
    (out,) = run(tmp_path, [book(**shelves)])
    assert {k: out[k] for k in shelves} == shelves
    assert 360 <= int(out["Number of Pages"]) <= 440


def test_seed_makes_runs_repeatable(tmp_path: Path) -> None:
    assert run(tmp_path, [book()]) == run(tmp_path, [book()])

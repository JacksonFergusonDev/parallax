#!/usr/bin/env bash
# Spike S1: how do collections behave for books the Kobo has not imported yet?
# See docs/hardware-checks.md for the full procedure. Subcommands:
#   prepare      back up DB, copy 3 test books, pre-assign A (direct row) and B (trigger)
#   check        read-only report of what Nickel did with the test books
#   replug-test  assign C (already imported) to a collection + series, the "replug" flow
#   cleanup      remove test books, collections, trigger and side table
#   restore      copy the newest backup back over the device DB (emergency only)
set -euo pipefail

VOL="${KOBO_VOLUME:-/Volumes/KOBOeReader}"
DB="$VOL/.kobo/KoboReader.sqlite"
DIR_NAME="ERC-Spike"
DEV_DIR="$VOL/$DIR_NAME"
CID_PREFIX="file:///mnt/onboard/$DIR_NAME"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BACKUPS="$REPO/.spike-backups"
NOW="strftime('%Y-%m-%dT%H:%M:%SZ','now')"

die() { echo "error: $*" >&2; exit 1; }

preflight() {
  [ -f "$VOL/.kobo/version" ] || die "no Kobo mounted at $VOL"
  [ -f "$DB" ] || die "missing $DB"
  for f in "$DB-wal" "$DB-shm" "$DB-journal"; do
    [ ! -e "$f" ] || die "$f exists; replug the Kobo and try again"
  done
}

# Apple's sqlite3 keeps -wal/-shm after close by default; Nickel expects them gone.
sql() { sqlite3 -bail -cmd ".output /dev/null" -cmd ".filectrl persist_wal 0" -cmd ".output stdout" "$DB" "$@"; }

# Leave the DB as Nickel expects it: WAL folded back in, no side files.
finish_write() {
  local ok
  ok="$(sql "PRAGMA wal_checkpoint(TRUNCATE); PRAGMA integrity_check;" | tail -1)"
  [ "$ok" = "ok" ] || die "integrity_check failed after write: $ok (run: $0 restore)"
  for f in "$DB-wal" "$DB-shm"; do
    [ ! -e "$f" ] || echo "warning: $f left behind" >&2
  done
}

backup() {
  mkdir -p "$BACKUPS"
  local dest
  dest="$BACKUPS/KoboReader-$(date +%Y%m%d-%H%M%S)-$1.sqlite"
  sql ".backup '$dest'"
  [ "$(sqlite3 "$dest" 'PRAGMA integrity_check;')" = "ok" ] || die "backup failed integrity check"
  echo "backup: $dest"
}

make_epub() { # id title out
  python3 -I - "$@" <<'PY'
import sys, zipfile
book_id, title, out = sys.argv[1:4]
opf = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title>{title}</dc:title><dc:creator>ERC Spike</dc:creator>
  <dc:language>en</dc:language><dc:identifier id="uid">urn:uuid:erc-spike-{book_id}</dc:identifier>
 </metadata>
 <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest>
 <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"""
ncx = f"""<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:uuid:erc-spike-{book_id}"/></head>
<docTitle><text>{title}</text></docTitle><navMap><navPoint id="p1" playOrder="1"><navLabel><text>Start</text></navLabel><content src="c1.xhtml"/></navPoint></navMap></ncx>"""
body = f"""<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head>
<body><h1>{title}</h1><p>Test book for spike S1. Safe to delete.</p></body></html>"""
container = """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"""
with zipfile.ZipFile(out, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    z.writestr("META-INF/container.xml", container, compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("content.opf", opf, compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("toc.ncx", ncx, compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("c1.xhtml", body, compress_type=zipfile.ZIP_DEFLATED)
PY
}

add_shelf() { # name
  echo "INSERT OR IGNORE INTO Shelf (CreationDate, InternalName, LastModified, Name, _IsDeleted, _IsVisible, _IsSynced, Id, Type)
        VALUES ($NOW, '$1', $NOW, '$1', 'false', 'true', 'false', '$1', 'UserTag');"
}

add_member() { # shelf contentid
  echo "INSERT OR IGNORE INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced)
        VALUES ('$1', '$2', $NOW, 'false', 'false');"
}

cmd_prepare() {
  preflight
  [ ! -e "$DEV_DIR" ] || die "$DEV_DIR already exists; run cleanup first"
  backup prepare
  local tmp
  tmp="$(mktemp -d)"
  make_epub a "ERC Spike A" "$tmp/spike-a.epub"
  make_epub b "ERC Spike B" "$tmp/spike-b.epub"
  make_epub c "ERC Spike C" "$tmp/spike-c.epub"
  mkdir "$DEV_DIR"
  for f in "$tmp"/*.epub; do cp -X "$f" "$DEV_DIR/"; done
  rm -rf "$tmp"
  find "$DEV_DIR" -name '._*' -delete
  touch "$VOL/.metadata_never_index"

  sql <<SQL
BEGIN;
$(add_shelf 'ERC Spike A')
$(add_shelf 'ERC Spike B')
-- A: membership row written before the book exists in content.
$(add_member 'ERC Spike A' "$CID_PREFIX/spike-a.epub")
-- B: membership written by a trigger at the moment Nickel imports the book.
CREATE TABLE IF NOT EXISTS _erc_pending_shelf (ContentId TEXT, ShelfName TEXT, PRIMARY KEY (ContentId, ShelfName));
INSERT OR IGNORE INTO _erc_pending_shelf VALUES ('$CID_PREFIX/spike-b.epub', 'ERC Spike B');
CREATE TRIGGER IF NOT EXISTS _erc_content_insert AFTER INSERT ON content
  WHEN EXISTS (SELECT 1 FROM _erc_pending_shelf WHERE ContentId = new.ContentID)
BEGIN
  INSERT OR IGNORE INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced)
    SELECT ShelfName, new.ContentID, $NOW, 'false', 'false' FROM _erc_pending_shelf WHERE ContentId = new.ContentID;
  DELETE FROM _erc_pending_shelf WHERE ContentId = new.ContentID;
END;
COMMIT;
SQL
  finish_write
  echo "prepared. Next: follow 'Round 1' in docs/hardware-checks.md (eject with: diskutil eject \"$VOL\")"
}

cmd_check() {
  [ -f "$DB" ] || die "no Kobo mounted at $VOL"
  local copy
  copy="$(mktemp -d)/k.sqlite"
  cp "$DB" "$copy"   # read a copy so nothing touches the device
  echo "== test books in content"
  sqlite3 -header -column "$copy" "SELECT substr(ContentID, length('$CID_PREFIX/')+1) AS file, Title, Series, SeriesNumber FROM content WHERE ContentType=6 AND ContentID LIKE '$CID_PREFIX/%' ORDER BY 1"
  echo "== test collections"
  sqlite3 -header -column "$copy" "SELECT Name, _IsDeleted, _IsSynced FROM Shelf WHERE Name LIKE 'ERC Spike%'"
  echo "== memberships"
  sqlite3 -header -column "$copy" "SELECT ShelfName, substr(ContentId, length('$CID_PREFIX/')+1) AS file, _IsDeleted, (SELECT count(*) FROM content c WHERE c.ContentID = sc.ContentId) AS imported FROM ShelfContent sc WHERE ShelfName LIKE 'ERC Spike%'"
  echo "== pending (trigger side table; empty after B imported)"
  sqlite3 "$copy" "SELECT * FROM _erc_pending_shelf" 2>/dev/null || echo "(no side table)"
  echo "== ghost ._ rows"
  sqlite3 "$copy" "SELECT count(*) FROM content WHERE ContentID LIKE '%/._%'"
  rm -rf "$(dirname "$copy")"
}

cmd_replug_test() {
  preflight
  local cid="$CID_PREFIX/spike-c.epub"
  [ "$(sql "SELECT count(*) FROM content WHERE ContentID='$cid'")" = "1" ] || die "book C not imported yet; eject, wait for import, replug"
  backup replug
  sql <<SQL
BEGIN;
$(add_shelf 'ERC Spike C')
$(add_member 'ERC Spike C' "$cid")
UPDATE content SET Series='ERC Spike Series', SeriesNumber='1', SeriesID='ERC Spike Series' WHERE ContentID='$cid' AND ContentType=6;
COMMIT;
SQL
  finish_write
  echo "done. Next: follow 'Round 2' in docs/hardware-checks.md"
}

cmd_cleanup() {
  preflight
  backup cleanup
  # Shelves were never synced (_IsSynced='false'), so removing the rows is safe.
  # Book rows are left for Nickel to drop when it sees the files are gone.
  sql <<SQL
BEGIN;
DROP TRIGGER IF EXISTS _erc_content_insert;
DROP TABLE IF EXISTS _erc_pending_shelf;
DELETE FROM ShelfContent WHERE ShelfName LIKE 'ERC Spike%';
DELETE FROM Shelf WHERE Name LIKE 'ERC Spike%';
COMMIT;
SQL
  finish_write
  rm -rf "$DEV_DIR"
  echo "cleaned up. Eject; the test books should vanish from the library after import."
}

cmd_restore() {
  [ -f "$VOL/.kobo/version" ] || die "no Kobo mounted at $VOL"
  local latest
  latest="$(ls -t "$BACKUPS"/KoboReader-*.sqlite 2>/dev/null | head -1)"
  [ -n "$latest" ] || die "no backups in $BACKUPS"
  read -r -p "Overwrite device DB with $latest? [y/N] " ans
  [ "$ans" = "y" ] || exit 1
  rm -f "$DB-wal" "$DB-shm"
  cp -X "$latest" "$DB"
  echo "restored."
}

case "${1:-}" in
  prepare) cmd_prepare ;;
  check) cmd_check ;;
  replug-test) cmd_replug_test ;;
  cleanup) cmd_cleanup ;;
  restore) cmd_restore ;;
  *) sed -n '2,9p' "$0"; exit 1 ;;
esac

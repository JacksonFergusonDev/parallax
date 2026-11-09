#!/bin/sh
# Answers pkg-config for libarchive with the copy built into macOS
# (/usr/lib/libarchive.2.dylib, linked through the SDK's libarchive.tbd), so
# reading CBR and CB7 comics needs nothing bundled or installed. Other
# packages go to a real pkg-config when there is one. Set as PKG_CONFIG in
# .cargo/config.toml.
case " $* " in
*" libarchive"*)
  case " $* " in
  *" --modversion "*) echo "3.7.4" ;;
  *" --libs"*) echo "-larchive" ;;
  esac
  exit 0
  ;;
esac
for candidate in /opt/homebrew/bin/pkg-config /usr/local/bin/pkg-config; do
  [ -x "$candidate" ] && exec "$candidate" "$@"
done
exit 1

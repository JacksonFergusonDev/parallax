#!/bin/sh
# macOS ships libarchive and links it through the SDK's libarchive.tbd.
# On other platforms and for other libraries, query the system pkg-config.
if [ "$(uname -s)" = Darwin ]; then
  case " $* " in
  *" libarchive"*)
    case " $* " in
    *" --modversion "*) echo "3.7.4" ;;
    *" --libs"*) echo "-larchive" ;;
    esac
    exit 0
    ;;
  esac
fi
exec pkg-config "$@"

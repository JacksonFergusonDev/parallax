#!/usr/bin/env bash
# Keep setup outside the retry loop; only restart Tauri after a Vite port conflict.
set -uo pipefail
cd "$(dirname "$0")/../app" || exit 1

log=$(mktemp) || exit 1
trap 'rm -f "$log"' EXIT

listeners() {
    lsof -nP -t -iTCP:1420 -sTCP:LISTEN
}

while true; do
    pnpm tauri dev 2>&1 | tee "$log"
    status=${PIPESTATUS[0]}
    if (( status == 0 )) || ! grep -Fq 'Error: Port 1420 is already in use' "$log"; then
        exit "$status"
    fi

    if ! command -v lsof >/dev/null 2>&1; then
        echo 'Cannot identify the process on port 1420: lsof is required.' >&2
        exit "$status"
    fi
    pids=$(listeners) || pids=''
    if [[ -z "$pids" ]]; then
        echo 'No listener found on port 1420. Run just dev again to retry.' >&2
        exit "$status"
    fi

    echo 'Port 1420 is used by:'
    while IFS= read -r pid; do
        # Validate lsof output before passing it to ps or kill.
        [[ "$pid" =~ ^[0-9]+$ ]] || exit "$status"
        ps -p "$pid" -o pid=,comm=
    done <<< "$pids"
    printf 'Kill these processes and retry app startup? [y/N] '
    IFS= read -r answer || exit "$status"
    case "$answer" in
        y|Y|yes|YES) ;;
        *) exit "$status" ;;
    esac

    # Only signal approved PIDs that still own the port; never kill a new owner.
    current=$(listeners) || current=''
    while IFS= read -r pid; do
        if grep -Fxq "$pid" <<< "$current"; then
            if ! kill -TERM "$pid"; then
                echo "Could not stop process $pid. App startup cancelled." >&2
                exit "$status"
            fi
        fi
    done <<< "$pids"

    # Give graceful shutdown up to five seconds; do not escalate to SIGKILL.
    for (( attempt = 0; attempt < 50; attempt++ )); do
        current=$(listeners) || current=''
        [[ -z "$current" ]] && break
        sleep 0.1
    done
    if [[ -n "$current" ]]; then
        echo 'Port 1420 is still in use. App startup cancelled.' >&2
        exit "$status"
    fi
    echo 'Port 1420 is free. Retrying app startup…'
done

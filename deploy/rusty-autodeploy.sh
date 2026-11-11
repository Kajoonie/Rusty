#!/usr/bin/env bash
#
# rusty-autodeploy.sh - deploy the latest release build of the Rusty bot.
#
# Run every few minutes by rusty-autodeploy.timer on the deployment machine.
# It compares the checksum published with the rolling "latest" GitHub release
# against the binary that is deployed. When they differ it downloads the new
# binary, verifies it, swaps it in, restarts the bot and rolls back if the bot
# does not stay up.
#
# Installed by hand as /usr/local/sbin/rusty-autodeploy.sh (see the
# "Deployment" section of README.md), so a change to this file in the repository
# does not change what runs as root until it is copied again.
#
# Needs: bash, curl, coreutils, flock, ldd, systemctl; git only for the optional
# checkout fast-forward. No jq, no GitHub API, no token.
#
# Exit status: 0  nothing to do, or deployed
#              75 could not check or download right now; the next run retries
#              1  a build was rejected or rolled back, or the setup is broken

set -Eeuo pipefail

# --- Settings. Each can be overridden from the environment, for example in
# --- /etc/default/rusty-autodeploy, which the systemd unit reads.
REPO="${RUSTY_AUTODEPLOY_REPO:-Kajoonie/Rusty}"
RELEASE_TAG="${RUSTY_AUTODEPLOY_TAG:-latest}"
BASE_URL="${RUSTY_AUTODEPLOY_BASE_URL:-https://github.com/${REPO}/releases/download/${RELEASE_TAG}}"
APP_DIR="${RUSTY_AUTODEPLOY_APP_DIR:-/root/Rusty}"
BIN="${RUSTY_AUTODEPLOY_BIN:-${APP_DIR}/target/release/rusty}"
UNIT="${RUSTY_AUTODEPLOY_UNIT:-rusty.service}"
STATE_DIR="${RUSTY_AUTODEPLOY_STATE_DIR:-/var/lib/rusty-autodeploy}"
# Seconds to wait after a restart before judging whether the bot stayed up.
HEALTH_WAIT="${RUSTY_AUTODEPLOY_HEALTH_WAIT:-20}"
# 1: after a deploy, fast-forward the checkout in APP_DIR to the deployed
# commit when that is safe. 0: never touch the checkout.
SYNC_CHECKOUT="${RUSTY_AUTODEPLOY_SYNC_CHECKOUT:-1}"
BRANCH="${RUSTY_AUTODEPLOY_BRANCH:-main}"

ASSET_BIN="rusty"
ASSET_SHA="rusty.sha256"
ASSET_INFO="BUILD_INFO"

PREV="${BIN}.prev"
HOLD_FILE="${STATE_DIR}/hold"
EX_TEMPFAIL=75

# --- Logging: one short line per action. Under systemd the "<N>" prefix sets
# --- the journal priority (5 notice, 4 warning, 3 error).
if [ -n "${JOURNAL_STREAM:-}" ]; then
    log()  { printf '<5>%s\n' "$*"; }
    warn() { printf '<4>%s\n' "$*"; }
    err()  { printf '<3>%s\n' "$*"; }
else
    log()  { printf '%s\n' "$*" >&2; }
    warn() { printf 'warning: %s\n' "$*" >&2; }
    err()  { printf 'error: %s\n' "$*" >&2; }
fi

TMP_DIR=""
NEW_BIN=""
# shellcheck disable=SC2329  # invoked through the EXIT trap
cleanup() {
    [ -n "$NEW_BIN" ] && rm -f -- "$NEW_BIN"
    [ -n "$TMP_DIR" ] && rm -rf -- "$TMP_DIR"
    return 0
}
trap cleanup EXIT
# Anything that fails without being handled below ends the run; say where.
trap 'err "unexpected failure at line ${LINENO} (exit status $?); stopping"' ERR

# Only allow https when talking to the real thing; a plain http base URL is
# accepted so the script can be tested against a local server.
CURL_PROTO=()
case "$BASE_URL" in
    https://*) CURL_PROTO=(--proto '=https' --proto-redir '=https') ;;
esac

FETCH_ERR=""
# fetch <asset> <destination> <max seconds>
# Returns 0 on success, 44 when the asset does not exist (HTTP 404) and 1 on
# any other failure, with a short reason in FETCH_ERR.
fetch() {
    local asset="$1" dest="$2" max_time="$3" code
    FETCH_ERR=""
    if ! code="$(curl -sSL "${CURL_PROTO[@]}" --connect-timeout 15 --max-time "$max_time" \
        --retry 2 -o "$dest" -w '%{http_code}' "${BASE_URL}/${asset}" 2>"${TMP_DIR}/curl.err")"; then
        FETCH_ERR="$(head -n 1 "${TMP_DIR}/curl.err")"
        return 1
    fi
    case "$code" in
        200) return 0 ;;
        404) FETCH_ERR="HTTP 404"; return 44 ;;
        *)   FETCH_ERR="HTTP ${code}"; return 1 ;;
    esac
}

sha_of() { sha256sum -- "$1" | cut -d' ' -f1; }
short()  { printf '%s' "${1:0:12}"; }

# Value of "key=value" from a BUILD_INFO style file; empty when absent.
info_value() {
    local key="$1" file="$2" line
    [ -r "$file" ] || return 0
    while IFS= read -r line; do
        case "$line" in
            "${key}="*) printf '%s' "${line#*=}"; return 0 ;;
        esac
    done < "$file"
    return 0
}

# x86_64 ELF: magic 7f 'E' 'L' 'F', 64-bit, little endian, e_machine 0x3e.
is_x86_64_elf() {
    local head
    head="$(od -An -tx1 -N20 -- "$1" | tr -d ' \n')"
    [ "${#head}" -eq 40 ] && [ "${head:0:12}" = "7f454c460201" ] && [ "${head:36:4}" = "3e00" ]
}

# Prints the reason and returns 1 when the dynamic loader cannot resolve
# everything the binary needs (missing library or too old a libc).
check_libraries() {
    local out
    if ! out="$(ldd -- "$1" 2>&1)"; then
        case "$out" in
            *"not a dynamic executable"*|*"statically linked"*) return 0 ;;
        esac
        printf 'ldd failed: %s' "$(printf '%s' "$out" | head -n 1)"
        return 1
    fi
    if printf '%s\n' "$out" | grep -q 'not found'; then
        printf '%s' "$(printf '%s\n' "$out" | grep 'not found' | head -n 3 | tr -s '\t\n ' ' ' | sed 's/^ //; s/ $//')"
        return 1
    fi
    return 0
}

unit_state()    { systemctl is-active "$UNIT" 2>/dev/null || true; }
unit_restarts() { systemctl show --property=NRestarts --value "$UNIT" 2>/dev/null || true; }

# Restart the bot and decide whether it stayed up: the unit must be active
# after HEALTH_WAIT seconds and systemd must not have restarted it meanwhile.
restart_and_check() {
    local before after state
    # Forget earlier failures first. A unit that just crash-looped has used up
    # its start limit, and systemd would refuse to start the binary we roll
    # back to ("Start request repeated too quickly").
    systemctl reset-failed "$UNIT" 2>/dev/null || true
    if ! systemctl restart "$UNIT"; then
        return 1
    fi
    before="$(unit_restarts)"
    sleep "$HEALTH_WAIT"
    state="$(unit_state)"
    after="$(unit_restarts)"
    [ "$state" = "active" ] && [ "$before" = "$after" ]
}

# Remember a build that must not be tried again. Runs stay silent about it
# until the release changes or the hold file is deleted.
hold_build() {
    printf '%s %s %s\n' "$1" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$2" > "$HOLD_FILE"
}

# Best effort: bring the git checkout in APP_DIR to the deployed commit so the
# source next to the binary matches it. Only a clean fast-forward of BRANCH is
# done; anything else is left alone with one log line.
sync_checkout() {
    local commit="$1" head branch
    [ "$SYNC_CHECKOUT" = "1" ] || return 0
    case "$commit" in
        *[!0-9a-f]*|"") return 0 ;;
    esac
    [ "${#commit}" -eq 40 ] || return 0
    command -v git >/dev/null 2>&1 || return 0
    git -C "$APP_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1 || return 0

    head="$(git -C "$APP_DIR" rev-parse HEAD 2>/dev/null || true)"
    [ "$head" != "$commit" ] || return 0
    branch="$(git -C "$APP_DIR" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
    if [ "$branch" != "$BRANCH" ]; then
        log "checkout not updated: ${APP_DIR} is on '${branch:-a detached HEAD}', not ${BRANCH}"
        return 0
    fi
    if [ -n "$(git -C "$APP_DIR" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
        log "checkout not updated: ${APP_DIR} has local changes"
        return 0
    fi
    if ! GIT_TERMINAL_PROMPT=0 timeout 60 git -C "$APP_DIR" fetch --quiet origin "$BRANCH" 2>/dev/null; then
        log "checkout not updated: git fetch failed"
        return 0
    fi
    if ! git -C "$APP_DIR" cat-file -e "${commit}^{commit}" 2>/dev/null; then
        log "checkout not updated: commit $(short "$commit") not found on origin/${BRANCH}"
        return 0
    fi
    if ! git -C "$APP_DIR" merge-base --is-ancestor HEAD "$commit" 2>/dev/null; then
        log "checkout not updated: $(short "$commit") is not a fast-forward of the checkout"
        return 0
    fi
    if git -C "$APP_DIR" merge --ff-only --quiet "$commit" >/dev/null 2>&1; then
        log "checkout fast-forwarded to $(short "$commit")"
    else
        log "checkout not updated: fast-forward to $(short "$commit") failed"
    fi
    return 0
}

main() {
    local bin_dir remote_sha current_sha new_sha rc reason commit commit_date state info kept

    bin_dir="$(dirname -- "$BIN")"
    if [ ! -d "$bin_dir" ]; then
        err "binary directory ${bin_dir} does not exist; nothing deployed"
        exit 1
    fi
    mkdir -p -- "$STATE_DIR"

    # One run at a time.
    exec 9>"${STATE_DIR}/lock"
    if ! flock -n 9; then
        log "another run holds the lock; skipping this one"
        exit 0
    fi

    # Leftovers of a run that was killed half-way.
    rm -rf -- "${STATE_DIR}"/tmp.?????? "${BIN}".new.??????
    TMP_DIR="$(mktemp -d "${STATE_DIR}/tmp.XXXXXX")"

    # 1. What does the release say?
    rc=0
    fetch "$ASSET_SHA" "${TMP_DIR}/${ASSET_SHA}" 60 || rc=$?
    if [ "$rc" -eq 44 ]; then
        log "no release found at ${BASE_URL} yet; nothing to do"
        exit 0
    elif [ "$rc" -ne 0 ]; then
        warn "could not fetch ${ASSET_SHA} (${FETCH_ERR}); will retry"
        exit "$EX_TEMPFAIL"
    fi
    remote_sha=""
    IFS=' ' read -r remote_sha _ < "${TMP_DIR}/${ASSET_SHA}" || true
    if ! [[ "$remote_sha" =~ ^[0-9a-f]{64}$ ]]; then
        warn "${ASSET_SHA} from the release is not a sha256 checksum; will retry"
        exit "$EX_TEMPFAIL"
    fi

    # 2. Is that what is deployed? Then stay silent.
    current_sha="none"
    if [ -f "$BIN" ]; then
        current_sha="$(sha_of "$BIN")"
    fi
    if [ "$remote_sha" = "$current_sha" ]; then
        exit 0
    fi
    if [ -r "$HOLD_FILE" ] && [ "$(cut -d' ' -f1 "$HOLD_FILE" 2>/dev/null)" = "$remote_sha" ]; then
        exit 0
    fi

    # 3. Download next to the target (same filesystem, so the swap is a rename)
    #    and verify before anything is touched.
    NEW_BIN="$(mktemp "${BIN}.new.XXXXXX")"
    rc=0
    fetch "$ASSET_BIN" "$NEW_BIN" 600 || rc=$?
    if [ "$rc" -ne 0 ]; then
        warn "release announces build $(short "$remote_sha") but downloading ${ASSET_BIN} failed (${FETCH_ERR}); will retry"
        exit "$EX_TEMPFAIL"
    fi
    new_sha="$(sha_of "$NEW_BIN")"
    if [ "$new_sha" != "$remote_sha" ]; then
        warn "checksum mismatch: ${ASSET_SHA} says $(short "$remote_sha"), downloaded ${ASSET_BIN} is $(short "$new_sha") (upload in progress?); will retry"
        exit "$EX_TEMPFAIL"
    fi
    chmod 0755 -- "$NEW_BIN"
    if ! is_x86_64_elf "$NEW_BIN"; then
        hold_build "$remote_sha" "not an x86_64 ELF binary"
        err "build $(short "$remote_sha") rejected: not an x86_64 ELF binary; keeping the current binary (held until the release changes or ${HOLD_FILE} is deleted)"
        exit 1
    fi
    if ! reason="$(check_libraries "$NEW_BIN")"; then
        hold_build "$remote_sha" "$reason"
        err "build $(short "$remote_sha") rejected: ${reason}; keeping the current binary (held until the release changes or ${HOLD_FILE} is deleted)"
        exit 1
    fi

    # Build details are only trusted when they describe this exact binary.
    commit=""
    commit_date=""
    info="${TMP_DIR}/${ASSET_INFO}"
    if fetch "$ASSET_INFO" "$info" 60 && [ "$(info_value sha256 "$info")" = "$remote_sha" ]; then
        commit="$(info_value commit "$info")"
        commit_date="$(info_value commit_date "$info")"
    else
        printf 'sha256=%s\n' "$remote_sha" > "$info"
    fi

    # 4. Swap: keep the running binary as rusty.prev, rename the new one in.
    state="$(unit_state)"
    if [ -f "$BIN" ]; then
        cp -p -- "$BIN" "${PREV}.tmp"
        mv -fT -- "${PREV}.tmp" "$PREV"
    fi
    mv -fT -- "$NEW_BIN" "$BIN"
    NEW_BIN=""

    # 5. Restart and watch. A bot that was stopped on purpose stays stopped.
    if [ "$state" = "inactive" ]; then
        log "${UNIT} is stopped; installed build $(short "$remote_sha") without starting it"
    elif ! restart_and_check; then
        hold_build "$remote_sha" "did not stay up after restart"
        if [ "$current_sha" != "none" ] && [ -f "$PREV" ]; then
            cp -p -- "$PREV" "${BIN}.rollback"
            mv -fT -- "${BIN}.rollback" "$BIN"
            if restart_and_check; then
                err "build $(short "$remote_sha") did not stay up; rolled back to $(short "$current_sha"), ${UNIT} is running again (build held until the release changes or ${HOLD_FILE} is deleted)"
            else
                err "build $(short "$remote_sha") did not stay up; rolled back to $(short "$current_sha") but ${UNIT} is still not healthy: check 'systemctl status ${UNIT}'"
            fi
        else
            err "build $(short "$remote_sha") did not stay up and there is no previous binary to roll back to: check 'systemctl status ${UNIT}'"
        fi
        exit 1
    fi

    # 6. Record what is deployed.
    printf 'deployed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$info"
    mv -fT -- "$info" "${STATE_DIR}/${ASSET_INFO}"
    rm -f -- "$HOLD_FILE"
    if [ "$current_sha" = "none" ]; then
        kept="there was no previous binary"
    else
        kept="previous binary $(short "$current_sha") kept as ${PREV}"
    fi
    log "deployed build $(short "$remote_sha")${commit:+ (commit $(short "$commit")${commit_date:+, ${commit_date}})}; ${kept}"

    sync_checkout "$commit"
    exit 0
}

main "$@"

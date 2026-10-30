#!/usr/bin/env bash
# scripts/run_upstream_tests.sh
# Thin wrapper to execute the upstream rsync testsuite against ssync.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
UPSTREAM_SRCDIR="/f/rsync"
TOOLDIR="$WORKSPACE_ROOT/target/upstream-tooldir"
SSYNC_BIN="${SSYNC_BIN:-$WORKSPACE_ROOT/target/debug/ssync}"

# 1. Ensure ssync is built
if [ ! -f "$SSYNC_BIN" ]; then
    echo "==> Building ssync ($SSYNC_BIN)..."
    cargo build --bin ssync
fi

# 2. Ensure upstream C helpers are built out-of-tree in target/upstream-tooldir
mkdir -p "$TOOLDIR"
if [ ! -f "$TOOLDIR/tls" ] || [ ! -f "$TOOLDIR/trimslash" ]; then
    echo "==> Configuring and building upstream test helpers in $TOOLDIR..."
    (
        cd "$TOOLDIR"
        "$UPSTREAM_SRCDIR/configure" --disable-md2man
        make -j"$(nproc)" tls getgroups getfsdev testrun trimslash t_unsafe t_chmod_secure \
            t_rename_secure t_symlink_secure t_secure_relpath t_acl t_hashtable_overflow \
            t_iwildmatch t_clean_fname t_safe_arg wildtest simdtest
    )
fi

# 3. Create a symlink named 'rsync' in $TOOLDIR pointing to ssync
# (This satisfies tests that hardcode `./rsync` or `TOOLDIR/rsync`)
ln -sf "$SSYNC_BIN" "$TOOLDIR/rsync"

echo "==> Running upstream testsuite with binary: $SSYNC_BIN"
python3 "$UPSTREAM_SRCDIR/runtests.py" \
    --srcdir="$UPSTREAM_SRCDIR" \
    --tooldir="$TOOLDIR" \
    --rsync-bin="$SSYNC_BIN" \
    "$@"

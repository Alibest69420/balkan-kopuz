# Everything runs inside the flake: a bare binary dies on NoWaylandLib.
nix := "nix develop --command"
daemon_dir := justfile_directory() / ".." / "kopuz"
socket := env_var_or_default("KOPUZ_SOCKET", env_var("XDG_RUNTIME_DIR") / "kopuzd-dev.sock")

_default:
    @just --list

# Build the frontend, and the daemon beside it.
build profile="debug":
    {{ nix }} cargo build {{ if profile == "release" { "--release" } else { "" } }}
    cd {{ daemon_dir }} && {{ nix }} cargo build -p kopuz-kopuzd {{ if profile == "release" { "--release" } else { "" } }}

# Run the app against its own daemon; neither outlives the other.
run profile="debug" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    just build {{ profile }}
    export KOPUZ_SOCKET={{ socket }}
    rm -f "$KOPUZ_SOCKET"

    # One process group, so a kill reaches the whole tree rather than the wrapper
    # `nix develop` leaves in front of each binary.
    set -m
    {{ nix }} {{ daemon_dir }}/target/{{ profile }}/kopuzd --socket "$KOPUZ_SOCKET" &
    daemon=$!

    # The socket is the daemon saying it is ready; starting the app before it exists
    # just shows the connection screen for a second.
    for _ in $(seq 1 100); do
        [ -S "$KOPUZ_SOCKET" ] && break
        kill -0 $daemon 2>/dev/null || { echo "the daemon exited before it listened"; exit 1; }
        sleep 0.2
    done

    {{ nix }} ./target/{{ profile }}/kopuz-gpui {{ args }} &
    app=$!

    # Whichever goes first takes the other with it: a frontend with no daemon can only
    # show a disconnect, and a daemon with no frontend is a process nobody asked for.
    stop() { kill -- -$daemon -$app 2>/dev/null || true; }
    trap stop EXIT INT TERM
    wait -n $daemon $app
    stop
    wait 2>/dev/null || true

# Run it and bring the frontend back if it dies while the daemon is still healthy.
watch profile="debug" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    just build {{ profile }}
    export KOPUZ_SOCKET={{ socket }}
    rm -f "$KOPUZ_SOCKET"
    set -m
    {{ nix }} {{ daemon_dir }}/target/{{ profile }}/kopuzd --socket "$KOPUZ_SOCKET" &
    daemon=$!
    trap 'kill -- -$daemon 2>/dev/null || true' EXIT INT TERM
    for _ in $(seq 1 100); do
        [ -S "$KOPUZ_SOCKET" ] && break
        kill -0 $daemon 2>/dev/null || { echo "the daemon exited before it listened"; exit 1; }
        sleep 0.2
    done
    # A crash loop is worth seeing rather than hiding, so a fast second death stops here.
    while kill -0 $daemon 2>/dev/null; do
        started=$SECONDS
        {{ nix }} ./target/{{ profile }}/kopuz-gpui {{ args }} || true
        kill -0 $daemon 2>/dev/null || break
        if [ $(( SECONDS - started )) -lt 5 ]; then
            echo "the frontend died within five seconds; not restarting it again"
            break
        fi
        echo "the frontend exited; bringing it back"
    done

# Everything CI checks, in the order that fails fastest.
check:
    {{ nix }} cargo fmt --all -- --check
    {{ nix }} cargo clippy --workspace --all-targets -j 4 -- -D warnings
    {{ nix }} cargo test --workspace -j 4

fmt:
    {{ nix }} cargo fmt --all

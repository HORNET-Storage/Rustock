#!/bin/bash
# Turns the RUSTOCK_* environment into rustock-cli flags, then execs the node
# so it stays PID 1.

set -euo pipefail

fail() {
    echo "entrypoint: $*" >&2
    exit 1
}

is_port() {
    [[ "$1" =~ ^[0-9]+$ ]] && [ "$1" -ge 1 ] && [ "$1" -le 65535 ]
}

is_ipv4() {
    [[ "$1" =~ ^(0|[1-9][0-9]{0,2})(\.(0|[1-9][0-9]{0,2})){3}$ ]] || return 1
    local octet
    for octet in ${1//./ }; do
        [ "$octet" -le 255 ] || return 1
    done
}

data_dir=/var/lib/rustock
p2p_port="${RUSTOCK_P2P_PORT:-5050}"
discovery_port="${RUSTOCK_DISCOVERY_PORT:-5051}"
network_id="${RUSTOCK_NETWORK_ID:-30}"
external_ip="${RUSTOCK_EXTERNAL_IP:-}"
log_level="${RUSTOCK_LOG_LEVEL:-info}"
log_to_stdout="${RUSTOCK_LOG_TO_STDOUT:-true}"
extra_args="${RUSTOCK_EXTRA_ARGS:-}"

is_port "$p2p_port" || fail "RUSTOCK_P2P_PORT must be a port number, got '$p2p_port'"
is_port "$discovery_port" || fail "RUSTOCK_DISCOVERY_PORT must be a port number, got '$discovery_port'"
[ -z "$external_ip" ] || is_ipv4 "$external_ip" || fail "RUSTOCK_EXTERNAL_IP must be an IPv4 address, got '$external_ip'"
[[ "$network_id" =~ ^[0-9]+$ ]] || fail "RUSTOCK_NETWORK_ID must be a number, got '$network_id'"
[ -n "$log_level" ] || fail "RUSTOCK_LOG_LEVEL must not be empty"
case "$log_to_stdout" in
    true|false) ;;
    *) fail "RUSTOCK_LOG_TO_STDOUT must be true or false, got '$log_to_stdout'" ;;
esac

# The node binds discovery to the P2P port + 1 and cannot be told otherwise.
if [ "$discovery_port" -ne $((p2p_port + 1)) ]; then
    fail "RUSTOCK_DISCOVERY_PORT is $discovery_port but must be RUSTOCK_P2P_PORT + 1 = $((p2p_port + 1))"
fi

# Docker creates a missing bind-mount source owned by root.
if [ ! -w "$data_dir" ]; then
    fail "$data_dir is not writable by uid $(id -u); run: chown $(id -u):$(id -g) <RUSTOCK_DATA_DIR path>"
fi

args=(--config /etc/rustock/node.toml --port "$p2p_port" --network-id "$network_id" --log-level "$log_level")

if [ "$log_to_stdout" = true ]; then
    args+=(--log-to-stdout)
fi

if [ -n "$external_ip" ]; then
    args+=(--external-ip "$external_ip")
else
    echo "entrypoint: RUSTOCK_EXTERNAL_IP is not set; networks 30 and 31 look it up, others advertise 127.0.0.1" >&2
fi

# The node gives RUST_LOG precedence over --log-level.
if [ -n "${RUST_LOG:-}" ]; then
    echo "entrypoint: RUST_LOG is set and overrides RUSTOCK_LOG_LEVEL" >&2
fi

if [ -n "$extra_args" ]; then
    read -r -a extra <<< "$extra_args"
    args+=("${extra[@]}")
fi

echo "entrypoint: network $network_id, p2p tcp $p2p_port, discovery udp $discovery_port, log level $log_level" >&2
exec /usr/local/bin/rustock-cli "${args[@]}" "$@"

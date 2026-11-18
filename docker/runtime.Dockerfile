# Runs the prebuilt rustock-cli that docker-compose.yml mounts at /usr/local/bin.
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 rustock \
 && useradd --system --uid 10001 --gid rustock --home-dir /var/lib/rustock --shell /usr/sbin/nologin rustock \
 && install -d -o rustock -g rustock /var/lib/rustock \
 && install -d /etc/rustock

COPY --chmod=755 entrypoint.sh /usr/local/bin/entrypoint.sh

USER rustock
WORKDIR /var/lib/rustock
VOLUME /var/lib/rustock

# P2P tcp, discovery udp (P2P + 1), RPC, metrics. Defaults; see .env.example.
EXPOSE 5050/tcp 5051/udp 4444/tcp 9898/tcp

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

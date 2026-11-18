# xmas-host: the relay's own host (xmas-tui --host), run beside the relay.
# It keeps the public squares and the server villages open, does what the
# relay's console asks, and reports who's where. Built by the release
# workflow from that release's own Linux build (checked against its
# SHA256SUMS); no window, no sound, no ports of its own.
FROM debian:bookworm-slim
RUN useradd -u 10002 -M -s /usr/sbin/nologin host
COPY xmas-tui /usr/local/bin/xmas-tui
USER host
# the relay, on the compose network; HOST_TOKEN comes from .env
ENV XMAS_RELAY=ws://xmas-relay:8080/mqtt
CMD ["xmas-tui", "--host"]

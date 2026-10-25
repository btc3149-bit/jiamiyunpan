ARG BIN_DIR=amd64

FROM alpine:3.18

# Inherit build variables
ARG BIN_DIR

# Install required runtime dependencies using apk
# The binary is a statically linked musl build on rustls/ring, so it needs
# no OpenSSL runtime library.
RUN apk add --no-cache bash libgcc tini tzdata

COPY $BIN_DIR/jiamiyunpan /usr/local/bin

COPY package/docker/entrypoint.sh /opt/

# Run as a dedicated non-root user. UID/GID 1000 is fixed so bind-mounted
# DATA_DIR ownership can be prepared on the host (chown 1000:1000 ./data).
# tini is invoked via ENTRYPOINT and works fine as non-root (it only needs
# to forward signals and reap children).
RUN addgroup -S -g 1000 jiamiyunpan \
    && adduser -S -D -H -u 1000 -G jiamiyunpan jiamiyunpan \
    && mkdir -p /data \
    && chown jiamiyunpan:jiamiyunpan /data \
    && chmod 755 /opt/entrypoint.sh /usr/local/bin/jiamiyunpan

EXPOSE 5443/tcp
ENV HOST 0.0.0.0
ENV DATA_DIR "/data"
ENV RUST_LOG "jiamiyunpan=info,auth=info,error=info,entity=info,storage=info,context=info,util=info,cryptfns=info,actix_web=info"

# Declare the data volume so named volumes inherit the correct ownership
# and the data directory is documented at the image level.
VOLUME /data

# Busybox wget (built into alpine) supports --no-check-certificate, which is
# needed because the server generates a self-signed cert by default.
# SSL_DISABLED=true deployments will report unhealthy — override the
# healthcheck in that case.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD wget --no-check-certificate -q -O /dev/null https://localhost:5443/api/readiness || exit 1

# Use Tini to ensure that our application responds to CTRL-C when run in the
# foreground without the Docker argument "--init" (which is actually another
# way of activating Tini, but cannot be enabled from inside the Docker image).
USER jiamiyunpan
ENTRYPOINT ["/sbin/tini", "--", "/opt/entrypoint.sh"]
CMD ["/usr/local/bin/jiamiyunpan", "-a", "0.0.0.0", "-p", "5443"]


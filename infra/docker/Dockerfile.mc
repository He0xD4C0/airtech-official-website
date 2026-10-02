# MinIO client built from the pinned upstream source; the server image and this
# client publish as separate artifacts so each carries one upstream license.
ARG GOLANG_IMAGE=golang:1.24.2-bookworm
FROM ${GOLANG_IMAGE} AS build
ARG MC_TAG=RELEASE.2025-08-13T08-35-41Z
ARG MC_COMMIT=7394ce0dd2a80935aded936b09fa12cbb3cb8096
ARG GOPROXY=https://proxy.golang.org,direct
ARG TARGETOS
ARG TARGETARCH
ENV GOPROXY=${GOPROXY} \
    GOTOOLCHAIN=local \
    CGO_ENABLED=0
RUN set -eux; \
    git clone --quiet --depth 1 --branch "${MC_TAG}" https://github.com/minio/mc /src; \
    [ "$(git -C /src rev-parse HEAD)" = "${MC_COMMIT}" ] \
      || { echo "mc source revision does not match the pinned commit ${MC_COMMIT}" >&2; exit 1; }
WORKDIR /src
RUN --mount=type=cache,target=/go/pkg/mod \
    set -eu; \
    ldflags="$(MC_RELEASE=RELEASE go run buildscripts/gen-ldflags.go)"; \
    MC_RELEASE=RELEASE GOOS="${TARGETOS:-linux}" GOARCH="${TARGETARCH}" \
      go build -tags kqueue -trimpath --ldflags "$ldflags" -o /out/mc

FROM debian:bookworm-slim
# Bucket initialization runs this image through `/bin/sh -c`, so the runtime
# keeps a POSIX shell instead of a scratch/distroless base.
RUN set -eux; \
    apt-get update; \
    apt-get install --no-install-recommends -y ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    useradd --system --uid 10001 --home-dir /home/mc --create-home mc; \
    install -d -o 10001 -g 10001 /licenses
COPY --from=build /out/mc /usr/local/bin/mc
COPY --from=build /src/LICENSE /licenses/mc-LICENSE
LABEL org.opencontainers.image.title="airtekpower minio client" \
      org.opencontainers.image.source="https://github.com/minio/mc" \
      org.opencontainers.image.revision="7394ce0dd2a80935aded936b09fa12cbb3cb8096" \
      org.opencontainers.image.version="RELEASE.2025-08-13T08-35-41Z"
USER 10001
ENTRYPOINT ["/usr/local/bin/mc"]

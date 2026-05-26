# payments-api 2.3.1 — looks healthy, but is the WRONG build.
#
# Runs as a long-lived container exactly like checkout-web: it logs a
# health line and idles, so `docker run` and a shallow check both pass.
# The defect is baked into the image, not its behaviour — it was
# tagged as a release but built with the development profile. Only
# `docker inspect` reveals `NODE_ENV=development`, which is what the
# verify gate catches.
FROM alpine:3.20

# BUG: should be `production`. A stray override during the build
# pinned the development profile into the release image.
ENV NODE_ENV=development

LABEL com.aeris.service="payments-api"
LABEL com.aeris.version="2.3.1"

CMD ["sh", "-c", "echo '{\"service\":\"payments-api\",\"status\":\"ok\"}'; trap 'exit 0' TERM; while true; do sleep 1; done"]

# checkout-web 1.8.0 — a correctly built production image.
#
# Runs as a long-lived container: it logs a startup health line, then
# idles. The saga starts it detached (`docker run -d --name …`),
# gates it on `docker inspect`, and leaves it stopped (`docker stop`)
# for inspection — never `--rm`, so you can still see what happened.
# The TERM trap makes `docker stop` exit promptly and cleanly.
FROM alpine:3.20

ENV NODE_ENV=production

LABEL com.aeris.service="checkout-web"
LABEL com.aeris.version="1.8.0"

# Logs health then idles. It does NOT echo NODE_ENV — the profile is
# only observable via `docker inspect`.
CMD ["sh", "-c", "echo '{\"service\":\"checkout-web\",\"status\":\"ok\"}'; trap 'exit 0' TERM; while true; do sleep 1; done"]

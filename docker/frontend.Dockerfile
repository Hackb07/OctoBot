FROM node:22-bookworm-slim AS builder

WORKDIR /app/octobot-web
COPY octobot-web/package.json /app/octobot-web/package.json
COPY octobot-web/package-lock.json /app/octobot-web/package-lock.json
RUN npm ci
COPY octobot-web /app/octobot-web
RUN npm run build

FROM nginx:1.27-alpine
COPY docker/frontend.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/octobot-web/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD wget -qO- http://127.0.0.1:8080/

FROM node:22-bookworm-slim AS builder

WORKDIR /app/frontend
COPY frontend/package.json /app/frontend/package.json
COPY frontend/package-lock.json /app/frontend/package-lock.json
RUN npm ci
COPY frontend /app/frontend
RUN npm run build

FROM nginx:1.27-alpine
COPY docker/frontend.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/frontend/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD wget -qO- http://127.0.0.1:8080/

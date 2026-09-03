# Frontend: static build served by nginx (SPA fallback routing).
# Build context is the repository root (needs both frontend/ and
# infrastructure/docker/nginx.conf); the root .dockerignore keeps it lean.

FROM node:22-alpine AS build
WORKDIR /app
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

FROM nginx:alpine
COPY infrastructure/docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80

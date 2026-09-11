FROM node:24-alpine

WORKDIR /app

RUN npm install -g pnpm@10

COPY package.json pnpm-lock.yaml tsconfig.json ./
COPY knexfile.ts db.ts index.ts redis.ts ./
COPY api ./api
COPY controllers ./controllers
COPY middlewares ./middlewares
COPY migrations ./migrations
COPY seeds ./seeds

RUN pnpm install --frozen-lockfile

EXPOSE 8000

CMD ["sh", "-c", "pnpm exec knex migrate:latest --knexfile knexfile.ts --env docker && pnpm exec knex seed:run --knexfile knexfile.ts --env docker && pnpm exec tsx index.ts"]

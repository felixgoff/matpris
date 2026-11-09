# The Matpris server for the phone and watch apps, which need it on the internet
# (the desktop apps run their own copy). It searches the chains from wherever it runs:
# docker build -t matpris . && docker run -p 3000:3000 -e PUBLIC_MAPKIT_TOKEN=… matpris
# Serve it over HTTPS: the browser only offers "Use my location" to secure pages.
FROM oven/bun:1.4.3 AS build
WORKDIR /app
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile
COPY . .
RUN bun run build

FROM oven/bun:1.4.3
WORKDIR /app
COPY package.json bun.lock ./
# Playwright and Google Chrome for ICA's session cookie (scripts/ica-warm.mjs, headless).
RUN bun install --frozen-lockfile && bunx playwright install --with-deps chrome && rm -rf /root/.cache/ms-playwright/.links
COPY --from=build /app/build ./build
COPY scripts/ica-warm.mjs ./scripts/ica-warm.mjs
ENV HOST=0.0.0.0 PORT=3000 NODE_ENV=production
EXPOSE 3000
CMD ["bun", "build/index.js"]

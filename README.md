# PointFlow

A Next.js web app.

## Stack

- **Next.js 16** (App Router, `src/` dir, TypeScript)
- **Tailwind CSS v4**
- **Drizzle ORM** + **Neon** (serverless Postgres)

## Getting started

```bash
pnpm install
cp .env.example .env   # fill in DATABASE_URL (Neon connection string)
pnpm dev
```

Open http://localhost:3000.

## Database (Drizzle + Neon)

The schema lives in `src/db/schema.ts`; the client is `src/db/index.ts`.

```bash
pnpm db:generate   # generate SQL migrations from schema
pnpm db:migrate    # apply migrations
pnpm db:push       # push schema directly (dev)
pnpm db:studio     # open Drizzle Studio
```

## Project structure

```
src/
  app/        # Next.js routes (App Router)
  db/
    index.ts  # Drizzle client (Neon HTTP)
    schema.ts # table definitions
drizzle.config.ts
```

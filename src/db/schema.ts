import { pgTable, serial, text, timestamp } from "drizzle-orm/pg-core";

// Starter table — replace with PointFlow's real domain model.
export const items = pgTable("items", {
  id: serial("id").primaryKey(),
  name: text("name").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).defaultNow().notNull(),
});

export type Item = typeof items.$inferSelect;
export type NewItem = typeof items.$inferInsert;

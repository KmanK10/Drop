import { describe, expect, it } from "vitest";
import { daysLeft, formatWhen } from "../src/client/lib/format.ts";

describe("days left", () => {
  const day = 86_400_000;
  const ttl = 30 * day;
  const created = 1_000_000_000_000;
  const expires = created + 30 * day;

  it("stays quiet at 4 days and warns with the real time left under that", () => {
    expect(daysLeft(created, ttl, expires - 4 * day)).toBeNull();
    expect(daysLeft(created, ttl, expires - 4 * day + 1)).toBe("3 days left");
    expect(daysLeft(created, ttl, expires - 3 * day)).toBe("3 days left");
    expect(daysLeft(created, ttl, expires - 2 * day)).toBe("2 days left");
    expect(daysLeft(created, ttl, expires - day)).toBe("1 day left");
    expect(daysLeft(created, ttl, expires - day + 1)).toBe("Less than a day");
    expect(daysLeft(created, ttl, expires - 12 * 60 * 60 * 1000)).toBe("Less than a day");
    expect(daysLeft(created, ttl, expires)).toBe("0 days left");
    expect(daysLeft(created, ttl, expires + 1)).toBe("0 days left");
    expect(daysLeft(created, 0, expires - day)).toBeNull();
  });

  it("leaves the created-time age words alone", () => {
    const now = 1_700_000_000_000;
    expect(formatWhen(now, now)).toBe("just now");
    expect(formatWhen(now - 15_000, now)).toBe("15s ago");
    expect(formatWhen(now - 90_000, now)).toBe("2m ago");
    expect(formatWhen(now - 24 * 60 * 60 * 1000, now)).toBe("1d ago");
  });
});

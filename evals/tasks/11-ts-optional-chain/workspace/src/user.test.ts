import { describe, it, expect } from "vitest";
import { getUserName } from "./user";

describe("getUserName", () => {
  it("returns name when present", () => {
    expect(getUserName({ name: "Alice" })).toBe("Alice");
  });
  it("returns Anonymous when name missing", () => {
    expect(getUserName({})).toBe("Anonymous");
  });
  it("returns Anonymous when null", () => {
    expect(getUserName(null)).toBe("Anonymous");
  });
  it("returns Anonymous when undefined", () => {
    expect(getUserName(undefined)).toBe("Anonymous");
  });
});

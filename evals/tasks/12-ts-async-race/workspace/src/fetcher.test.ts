import { describe, it, expect } from "vitest";
import { fetchWithTimeout } from "./fetcher";

describe("fetchWithTimeout", () => {
  it("resolves when fetch completes before timeout", async () => {
    // Use a data URL to simulate an instant fetch
    const result = await fetchWithTimeout("data:text/plain,hello", 5000);
    expect(result).toBe("hello");
  });
  it("rejects on timeout", async () => {
    // Use a slow endpoint that will never respond within 1ms
    await expect(
      fetchWithTimeout("https://httpstat.us/200?sleep=5000", 1)
    ).rejects.toThrow(/timeout/i);
  });
});

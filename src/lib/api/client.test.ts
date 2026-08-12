import { describe, expect, it } from "vitest";

import { resolveDevApiBaseUrl } from "./client";

describe("resolveDevApiBaseUrl", () => {
  it("默认使用受限的本地 Dev API 地址", () => {
    expect(resolveDevApiBaseUrl()).toBe("http://127.0.0.1:17321/dev-api");
    expect(resolveDevApiBaseUrl("http://127.0.0.1:17322/dev-api/")).toBe(
      "http://127.0.0.1:17322/dev-api",
    );
    expect(resolveDevApiBaseUrl("http://localhost:17322/dev-api")).toBe(
      "http://localhost:17322/dev-api",
    );
    expect(resolveDevApiBaseUrl("http://[::1]:17322/dev-api")).toBe(
      "http://[::1]:17322/dev-api",
    );
  });

  it("拒绝将本地 Dev API 重定向到非回环地址或错误路径", () => {
    for (const value of [
      "https://127.0.0.1:17322/dev-api",
      "http://192.168.1.8:17322/dev-api",
      "http://127.0.0.1:17322/not-dev-api",
      "http://127.0.0.1:17322/dev-api?token=unsafe",
    ]) {
      expect(resolveDevApiBaseUrl(value)).toBe(
        "http://127.0.0.1:17321/dev-api",
      );
    }
  });
});

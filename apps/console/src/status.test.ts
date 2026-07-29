import { describe, expect, it } from "vitest";

import { bootstrapMessage } from "./status";

describe("bootstrapMessage", () => {
  it("明确声明 Controller 尚未配置", () => {
    expect(bootstrapMessage("controller-unconfigured")).toContain("不会伪造");
  });
});

import { describe, expect, it } from "vitest";

import { pageStateMessage, type PageDataState } from "./status";

describe("pageStateMessage", () => {
  it.each<[PageDataState, string]>([
    ["loading", "正在读取"],
    ["empty", "没有可显示"],
    ["error", "未能完成"],
    ["forbidden", "没有访问权限"],
  ])("为 %s 提供明确状态", (state, expected) => {
    expect(pageStateMessage(state)).toContain(expected);
  });
});

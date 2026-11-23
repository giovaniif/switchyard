import {expect, test} from "bun:test";
import {readOnly, restrictedProfile} from "./restricted";

test("a restricted session has no tools, MCP or settings; any other keeps the SDK's", () => {
  for (const permission of [undefined, "plan"]) {
    const start = {restricted: true, permission};
    expect(readOnly(start)).toBe(true);
    expect(restrictedProfile(start).tools).toEqual([]);
    expect(restrictedProfile(start).strictMcpConfig).toBe(true);
    expect(restrictedProfile(start).persistSession).toBe(false);
    expect(restrictedProfile(start).allowDangerouslySkipPermissions).toBe(false);
  }
  expect(readOnly({permission: "plan"})).toBe(false);
  expect(restrictedProfile({permission: "plan"})).toEqual({});
  expect(restrictedProfile({restricted: false, permission: "full"})).toEqual({});
});

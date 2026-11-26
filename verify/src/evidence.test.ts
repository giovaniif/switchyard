import { expect, test } from "bun:test";
import { pickArtifact } from "./evidence";

test("a bundle published twice fetches the artifact its proof recorded", () => {
  const art = { path: "p4-cmd.log", sha256: "61111ebca8ff" + "0".repeat(52) };
  const tree = [
    { path: "evidence-b7774df/9a0b1c2d3e4f-p4-cmd.log" },
    { path: "evidence-b7774df/61111ebca8ff-p4-cmd.log" },
    { path: "evidence-other/61111ebca8ff-p4-cmd.log" },
  ];
  expect(pickArtifact(tree, "evidence-b7774df/", art)?.path).toBe("evidence-b7774df/61111ebca8ff-p4-cmd.log");
  // Published once: the one file under the prefix, whatever its hash.
  expect(pickArtifact(tree.slice(0, 1), "evidence-b7774df/", art)?.path).toBe("evidence-b7774df/9a0b1c2d3e4f-p4-cmd.log");
});

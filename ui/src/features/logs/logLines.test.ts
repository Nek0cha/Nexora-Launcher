import { describe, expect, it } from "vitest";
import { filterLines, parseLog } from "./logLines";

const sample = [
  "[12:00:00 UTC] [main/INFO]: Loading",
  "[12:00:01 UTC] [Render thread/WARN]: Slow frame",
  "[12:00:02 UTC] [Render thread/ERROR]: Crash",
  "java.lang.IllegalStateException: boom",
  "\tat net.minecraft.Foo.bar(Foo.java:1)",
  "Caused by: java.io.IOException: disk",
  "[12:00:03] [Client thread/INFO]: old format",
  "plain output",
  "",
].join("\r\n");

describe("parseLog", () => {
  it("levels each line, and gives stack trace lines the level of the line before", () => {
    const lines = parseLog(sample);
    expect(lines.map((l) => l.level)).toEqual(["other", "warn", "error", "error", "error", "error", "other", "other"]);
    expect(lines[0]?.text).toBe("[12:00:00 UTC] [main/INFO]: Loading");
  });

  it("drops only the empty line after the last newline", () => {
    expect(parseLog("a\n\nb\n").map((l) => l.text)).toEqual(["a", "", "b"]);
    expect(parseLog("")).toEqual([]);
  });

  it("does not let a trace line after an INFO line look like an error", () => {
    const lines = parseLog("[1:00:00] [main/INFO]: ok\n\tat x");
    expect(lines[1]?.level).toBe("other");
  });
});

describe("filterLines", () => {
  const lines = parseLog(sample);

  it("keeps everything with no filter", () => {
    expect(filterLines(lines, "all", "  ")).toBe(lines);
  });

  it("keeps warnings and errors, with their stack traces", () => {
    expect(filterLines(lines, "warn", "").map((l) => l.text)).toEqual(lines.slice(1, 6).map((l) => l.text));
  });

  it("keeps only errors", () => {
    expect(filterLines(lines, "error", "").map((l) => l.level)).toEqual(["error", "error", "error", "error"]);
  });

  it("searches without regard to case, together with the level", () => {
    expect(filterLines(lines, "all", "SLOW").map((l) => l.text)).toEqual(["[12:00:01 UTC] [Render thread/WARN]: Slow frame"]);
    expect(filterLines(lines, "error", "slow")).toEqual([]);
  });
});

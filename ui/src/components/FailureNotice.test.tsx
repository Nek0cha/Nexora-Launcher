import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import { FailureNotice } from "./FailureNotice";

afterEach(cleanup);

describe("FailureNotice", () => {
  it("shows the per-XErr message", async () => {
    await i18n.changeLanguage("en");
    render(<FailureNotice error={{ code: "auth.xbox", params: { xerr: 2148916233 }, detail: null }} />);
    expect(screen.getByText(i18n.t("errors:auth.xbox_2148916233" as never))).toBeTruthy();
  });

  it("falls back to the numbered message for an unknown XErr", async () => {
    await i18n.changeLanguage("en");
    render(<FailureNotice error={{ code: "auth.xbox", params: { xerr: 1 }, detail: null }} />);
    expect(screen.getByText("Xbox error (1)")).toBeTruthy();
  });
});

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { LicensesDialog } from "./LicensesDialog";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("LicensesDialog", () => {
  it("loads the text when opened and shows it", async () => {
    const load = vi.fn().mockResolvedValue("MIT License\nCopyright (c) someone");
    render(<LicensesDialog isOpen onOpenChange={() => {}} load={load} />);
    expect(await screen.findByText(/Copyright \(c\) someone/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Third-party licenses" })).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("shows nothing and does not load when closed", () => {
    const load = vi.fn().mockResolvedValue("text");
    render(<LicensesDialog isOpen={false} onOpenChange={() => {}} load={load} />);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(load).not.toHaveBeenCalled();
  });

  it("shows a failure notice when loading fails", async () => {
    const load = vi.fn().mockRejectedValue(new Error("chunk failed"));
    render(<LicensesDialog isOpen onOpenChange={() => {}} load={load} />);
    expect(await screen.findByRole("alert")).toBeTruthy();
  });
});

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { LicensesDialog, type Licenses } from "./LicensesDialog";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const sample: Licenses = {
  entries: [
    { name: "alpha", version: "1.0.0", license: "MIT", text: 0 },
    { name: "beta", version: "2.1.0", license: "MIT", text: 0 },
    { name: "gamma", version: "0.3.0", license: "ISC", text: 1 },
  ],
  texts: ["MIT License\nCopyright (c) someone", "ISC License\nCopyright (c) other"],
};

describe("LicensesDialog", () => {
  it("loads the list when opened and shows only the names at first", async () => {
    const load = vi.fn().mockResolvedValue(sample);
    render(<LicensesDialog isOpen onOpenChange={() => {}} load={load} />);
    expect(await screen.findByRole("button", { name: /alpha/ })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Third-party licenses" })).toBeTruthy();
    expect(screen.queryByText(/Copyright \(c\) someone/)).toBeNull();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("opens the text of one package at a time", async () => {
    render(<LicensesDialog isOpen onOpenChange={() => {}} load={vi.fn().mockResolvedValue(sample)} />);
    fireEvent.click(await screen.findByRole("button", { name: /alpha/ }));
    expect(screen.getByText(/Copyright \(c\) someone/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /gamma/ }));
    expect(screen.queryByText(/Copyright \(c\) someone/)).toBeNull();
    expect(screen.getByText(/Copyright \(c\) other/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /gamma/ }));
    expect(screen.queryByText(/Copyright \(c\) other/)).toBeNull();
  });

  it("renders only the opened text for a long list", async () => {
    const entries = Array.from({ length: 600 }, (_, i) => ({ name: `pkg${i}`, version: "1.0.0", license: "MIT", text: 0 }));
    render(
      <LicensesDialog isOpen onOpenChange={() => {}} load={vi.fn().mockResolvedValue({ entries, texts: ["MIT\n".repeat(500)] })} />,
    );
    await screen.findByRole("button", { name: /pkg599/ });
    expect(document.querySelectorAll("pre")).toHaveLength(0);
  });

  it("shows nothing and does not load when closed", () => {
    const load = vi.fn().mockResolvedValue(sample);
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

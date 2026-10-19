import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { ToastRegion } from "./ToastRegion";
import { clearToasts, pushToast } from "./useToast";

function show(kind: "success" | "info" | "error", message: string) {
  act(() => pushToast({ kind, message }));
}

function advance(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

// 出ているトースト（消える動きの途中のものは数えない）
const visible = (text: string) => {
  const el = screen.queryByText(text);
  return el && !el.closest("[data-exiting]") ? el : null;
};

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.useFakeTimers();
  render(<ToastRegion />);
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.useRealTimers();
});

describe("ToastRegion", () => {
  it("success_disappears_after_5s", () => {
    show("success", "Saved");
    advance(4900);
    expect(visible("Saved")).toBeTruthy();
    advance(200);
    // 消える動きを終わらせる
    advance(1000);
    expect(visible("Saved")).toBeNull();
  });

  it("error_disappears_after_8s", () => {
    show("error", "Broke");
    advance(7900);
    expect(visible("Broke")).toBeTruthy();
    advance(200);
    advance(1000);
    expect(visible("Broke")).toBeNull();
  });

  it("hover_keeps_the_toast", () => {
    show("success", "Saved");
    advance(1000);
    fireEvent.mouseEnter(screen.getByText("Saved"));
    advance(10_000);
    expect(visible("Saved")).toBeTruthy();
    fireEvent.mouseLeave(screen.getByText("Saved"));
    // 残りの 4 秒で消える
    advance(3900);
    expect(visible("Saved")).toBeTruthy();
    advance(200);
    advance(1000);
    expect(visible("Saved")).toBeNull();
  });

  it("focus_keeps_the_toast", () => {
    show("error", "Broke");
    act(() => screen.getByRole("button", { name: "Close" }).focus());
    advance(20_000);
    expect(visible("Broke")).toBeTruthy();
  });

  it("fourth_toast_removes_the_oldest", () => {
    show("info", "one");
    show("info", "two");
    show("info", "three");
    show("info", "four");
    advance(1000);
    expect(visible("one")).toBeNull();
    expect(visible("two")).toBeTruthy();
    expect(visible("three")).toBeTruthy();
    expect(visible("four")).toBeTruthy();
  });

  it("error_uses_alert_role_and_success_uses_status_role", () => {
    show("error", "Broke");
    show("success", "Saved");
    show("info", "Note");
    expect(screen.getByRole("alert").textContent).toContain("Broke");
    const statuses = screen.getAllByRole("status").map((el) => el.textContent);
    expect(statuses).toHaveLength(2);
    expect(statuses.some((s) => s?.includes("Saved"))).toBe(true);
    expect(statuses.some((s) => s?.includes("Note"))).toBe(true);
  });

  it("close_button_dismisses", () => {
    show("error", "Broke");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    advance(1000);
    expect(visible("Broke")).toBeNull();
  });

  it("sits_in_a_labelled_region", () => {
    expect(screen.getByRole("region", { name: "Notifications" })).toBeTruthy();
  });
});

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
// 読み上げ用の領域に差し込んだ文は、画面に出ているトーストではないので除く
const visible = (text: string) =>
  screen.queryAllByText(text).find((el) => !el.closest("[data-exiting]") && !el.closest('[role="status"]')) ?? null;

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
    fireEvent.mouseEnter(visible("Saved")!);
    advance(10_000);
    expect(visible("Saved")).toBeTruthy();
    fireEvent.mouseLeave(visible("Saved")!);
    // 残りの 4 秒で消える
    advance(3900);
    expect(visible("Saved")).toBeTruthy();
    advance(200);
    advance(1000);
    expect(visible("Saved")).toBeNull();
  });

  it("focus_keeps_the_toast", () => {
    show("error", "Broke");
    act(() => screen.getByRole("button", { name: "Close notification" }).focus());
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
    // 成功と情報の読み上げ用の領域は、トーストが出る前から置いてある
    const status = screen.getByRole("status");
    expect(status.getAttribute("aria-live")).toBe("polite");
    expect(status.textContent).toBe("");
    show("error", "Broke");
    show("success", "Saved");
    expect(screen.getByRole("alert").textContent).toContain("Broke");
    expect(screen.getAllByRole("status")).toEqual([status]);
    expect(status.textContent).toBe("Saved");
    show("info", "Note");
    expect(status.textContent).toBe("Note");
    // 失敗は読み上げ用の領域に入れない（role="alert" で読み上げる）
    show("error", "Broke again");
    expect(status.textContent).toBe("Note");
  });

  it("announcement_clears_when_its_toast_is_gone", () => {
    show("success", "Saved");
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("Saved");
    fireEvent.click(screen.getByRole("button", { name: "Close notification" }));
    expect(status.textContent).toBe("");
  });

  it("closing_a_focused_toast_does_not_keep_others_paused", () => {
    show("error", "Broke");
    show("info", "Later");
    const close = screen.getAllByRole("button", { name: "Close notification" })[0]!;
    act(() => close.focus());
    // キーボードで閉じる（マウスはトーストの上に無い）
    fireEvent.keyDown(close, { key: "Enter" });
    fireEvent.keyUp(close, { key: "Enter" });
    // 閉じたトーストは消える動きに入り、フォーカスのあった要素は使えなくなる（blur は届かない）
    advance(1000);
    expect(visible("Broke")).toBeNull();
    show("success", "Next");
    advance(5100);
    advance(1000);
    expect(visible("Later")).toBeNull();
    expect(visible("Next")).toBeNull();
  });

  it("close_button_dismisses", () => {
    show("error", "Broke");
    fireEvent.click(screen.getByRole("button", { name: "Close notification" }));
    advance(1000);
    expect(visible("Broke")).toBeNull();
  });

  it("sits_in_a_labelled_region", () => {
    expect(screen.getByRole("region", { name: "Notifications" })).toBeTruthy();
  });
});

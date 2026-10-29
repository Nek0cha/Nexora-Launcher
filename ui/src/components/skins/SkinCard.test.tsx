import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SkinCard } from "./SkinCard";

afterEach(cleanup);

// 署名 + IHDR（幅 64、高さ h）だけの PNG の先頭
function png(h: number): string {
  const bytes = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 64, 0, 0, 0, h, 8, 6, 0, 0, 0];
  return `data:image/png;base64,${btoa(String.fromCharCode(...bytes))}`;
}

const base = { image: png(64), name: "Alex", selected: false, pending: false, disabled: false, onPress: () => {} };

describe("SkinCard", () => {
  it("is a button named by the skin", () => {
    render(<SkinCard {...base} />);
    expect(screen.getByRole("button", { name: "Alex" })).toBeTruthy();
  });

  it("calls onPress", () => {
    const onPress = vi.fn();
    render(<SkinCard {...base} onPress={onPress} />);
    fireEvent.click(screen.getByRole("button", { name: "Alex" }));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it("shows the check circle only when selected", () => {
    const { rerender } = render(<SkinCard {...base} />);
    expect(screen.queryByTestId("skin-card-check")).toBeNull();
    rerender(<SkinCard {...base} selected />);
    expect(screen.getByTestId("skin-card-check")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Alex" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("shows a progress mark and aria-busy only while pending", () => {
    const { rerender } = render(<SkinCard {...base} />);
    expect(screen.queryByTestId("skin-card-progress")).toBeNull();
    rerender(<SkinCard {...base} pending disabled />);
    expect(screen.getByTestId("skin-card-progress")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Alex" }).parentElement?.getAttribute("aria-busy")).toBe("true");
  });

  it("does not press when disabled", () => {
    const onPress = vi.fn();
    render(<SkinCard {...base} disabled onPress={onPress} />);
    fireEvent.click(screen.getByRole("button", { name: "Alex" }));
    expect(onPress).not.toHaveBeenCalled();
  });

  it("draws the doll from CSS layers: 12 for a 64x64 image, 7 for 64x32, none without an image", () => {
    const { rerender } = render(<SkinCard {...base} />);
    expect(screen.getAllByTestId("skin-card-part")).toHaveLength(12);
    rerender(<SkinCard {...base} image={png(32)} />);
    expect(screen.getAllByTestId("skin-card-part")).toHaveLength(7);
    rerender(<SkinCard {...base} image={null} />);
    expect(screen.queryAllByTestId("skin-card-part")).toHaveLength(0);
  });

  it("makes the slim arms 3 units wide", () => {
    const widthOfArm = (variant: "classic" | "slim") => {
      const { unmount } = render(<SkinCard {...base} variant={variant} />);
      const arm = screen.getAllByTestId("skin-card-part")[2] as HTMLElement; // 頭、胴の次が右腕
      const w = arm.style.width;
      unmount();
      return w;
    };
    expect(widthOfArm("classic")).toBe("calc(25% + 0.5px)");
    expect(widthOfArm("slim")).toBe("calc(18.75% + 0.5px)");
  });

  it("renders the menu next to the button, not inside it", () => {
    render(<SkinCard {...base} menu={<span data-testid="menu" />} />);
    const menu = screen.getByTestId("menu");
    expect(menu).toBeTruthy();
    expect(screen.getByRole("button", { name: "Alex" }).contains(menu)).toBe(false);
  });
});

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { InstanceChip } from "./InstanceChip";

beforeEach(async () => {
  await i18n.changeLanguage("ja");
});

afterEach(() => {
  cleanup();
});

describe("InstanceChip", () => {
  it("shows the name and a remove button named after the instance", async () => {
    const onRemove = vi.fn();
    render(<InstanceChip name="Survival" onRemove={onRemove} />);
    expect(screen.getByText("Survival")).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Survival を外す" }));
    });
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it("has no remove button when it cannot be removed", () => {
    render(<InstanceChip name="Creative" />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("starts a drag from the chip but not from its remove button", () => {
    const onDragStart = vi.fn();
    render(<InstanceChip name="Survival" onRemove={() => {}} onDragStart={onDragStart} />);
    fireEvent.pointerDown(screen.getByRole("button", { name: "Survival を外す" }), { button: 0 });
    expect(onDragStart).not.toHaveBeenCalled();
    fireEvent.pointerDown(screen.getByText("Survival"), { button: 0 });
    expect(onDragStart).toHaveBeenCalledTimes(1);
  });

  it("ignores the secondary mouse button", () => {
    const onDragStart = vi.fn();
    render(<InstanceChip name="Survival" onDragStart={onDragStart} />);
    fireEvent.pointerDown(screen.getByText("Survival"), { button: 2 });
    expect(onDragStart).not.toHaveBeenCalled();
  });
});

describe("InstanceChip layout stability", () => {
  const classOf = (dimmed: boolean) => {
    const { container } = render(<InstanceChip name="Survival" onDragStart={() => {}} dimmed={dimmed} />);
    const chip = container.firstElementChild as HTMLElement;
    cleanup();
    return chip.className.split(/\s+/).filter(Boolean);
  };

  it("a dimmed chip (the one being dragged) differs from a normal one only by opacity, so its place stays the same size", () => {
    const normal = classOf(false);
    const dimmed = classOf(true);
    expect(dimmed.filter((c) => !normal.includes(c))).toEqual(["opacity-40"]);
    expect(normal.filter((c) => !dimmed.includes(c))).toEqual([]);
  });
});

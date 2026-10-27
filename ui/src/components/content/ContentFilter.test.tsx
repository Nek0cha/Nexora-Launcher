import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { emptyFilter, type FilterState } from "../../features/content/contentFilter";
import i18n from "../../i18n";
import { sizeChangingStateClasses } from "../layoutStability";
import { ChipGroup, ContentFilter, FilterSkeleton } from "./ContentFilter";

function mount(
  props: { state?: FilterState; categories?: string[] | null; showStatus?: boolean; showSearch?: boolean } = {},
) {
  const onChange = vi.fn<(next: FilterState) => void>();
  render(
    <ContentFilter
      state={props.state ?? emptyFilter}
      onChange={onChange}
      categories={props.categories === undefined ? ["optimization", "adventure"] : props.categories}
      showStatus={props.showStatus ?? true}
      showSearch={props.showSearch}
    />,
  );
  return onChange;
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
});

describe("ContentFilter", () => {
  it("typing in the search box emits the new query", () => {
    const onChange = mount();
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter by name" }), { target: { value: "sod" } });
    expect(onChange).toHaveBeenLastCalledWith({ ...emptyFilter, query: "sod" });
  });

  it("can leave out the search box", () => {
    mount({ showSearch: false });
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.getByRole("toolbar", { name: "Client side" })).toBeTruthy();
  });

  it("toggling a category emits it", () => {
    const onChange = mount({ state: { ...emptyFilter, categories: ["adventure"] } });
    const group = screen.getByRole("toolbar", { name: "Categories" });
    const button = screen.getByRole("button", { name: "Optimization" });
    expect(group.contains(button)).toBe(true);
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Adventure" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(button);
    expect(onChange).toHaveBeenLastCalledWith({ ...emptyFilter, categories: ["adventure", "optimization"] });
  });

  it("shows an unknown category slug as it is", () => {
    mount({ categories: ["some-new-tag"] });
    expect(screen.getByRole("button", { name: "some-new-tag" })).toBeTruthy();
  });

  it("toggling a side emits it", () => {
    const onChange = mount();
    const group = screen.getByRole("toolbar", { name: "Server side" });
    fireEvent.click(within(group).getByRole("button", { name: "Unsupported" }));
    expect(onChange).toHaveBeenLastCalledWith({ ...emptyFilter, server: ["unsupported"] });
  });

  it("hides the category section when categories is null", () => {
    mount({ categories: null });
    expect(screen.queryByRole("toolbar", { name: "Categories" })).toBeNull();
    expect(screen.getByRole("toolbar", { name: "Client side" })).toBeTruthy();
  });

  it("hides the status section unless showStatus", () => {
    mount({ showStatus: false });
    expect(screen.queryByRole("toolbar", { name: "Status" })).toBeNull();
    cleanup();
    const onChange = mount({ showStatus: true });
    fireEvent.click(screen.getByRole("button", { name: "Dependency" }));
    expect(onChange).toHaveBeenLastCalledWith({ ...emptyFilter, status: ["dependency"] });
  });

  it("chips change only colour when selected: no size-changing state classes", () => {
    mount();
    for (const chip of screen.getAllByRole("button", { name: /Required|Optional|Unsupported/ })) {
      expect(sizeChangingStateClasses(chip.className)).toEqual([]);
    }
  });

  it("keeps a check slot in every chip so selecting does not change its width", () => {
    const { rerender } = render(
      <ChipGroup label="Side" options={[{ id: "a", label: "Alpha" }]} selected={[]} onChange={() => {}} />,
    );
    const slot = () => screen.getByRole("button", { name: "Alpha" }).querySelector("[data-check-slot]");
    const before = slot();
    expect(before).not.toBeNull();
    expect(before?.getAttribute("aria-hidden")).toBe("true");
    expect(before?.querySelector("svg")).toBeNull();
    const frame = before?.className;
    rerender(<ChipGroup label="Side" options={[{ id: "a", label: "Alpha" }]} selected={["a"]} onChange={() => {}} />);
    expect(slot()?.className).toBe(frame);
    expect(slot()?.querySelector("svg")).not.toBeNull();
  });

  it("the skeleton chips take the same size classes and label width as the real chips, so the filter does not change height", () => {
    const { container } = render(<FilterSkeleton status />);
    const skeletonChips = Array.from(container.querySelectorAll("[aria-hidden=true]")).filter((e) =>
      e.className.includes("rounded-full"),
    );
    // クライアント 3、サーバー 3、カテゴリ、状態 3
    expect(skeletonChips.length).toBeGreaterThan(9);
    const first = skeletonChips[0] as HTMLElement;
    for (const token of ["h-7", "px-2.5", "gap-1", "text-xs", "whitespace-nowrap", "border"]) {
      expect(first.className.split(/\s+/)).toContain(token);
    }
    expect(first.querySelector('[class~="size-3.5"]')).not.toBeNull();
    expect(first.textContent).toBe("Required");
  });
});

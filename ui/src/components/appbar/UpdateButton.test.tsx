import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateState } from "../../features/update/updateStore";
import i18n from "../../i18n";
import { UpdateButton } from "./UpdateButton";

let state: UpdateState = { kind: "idle" };
let running: unknown[] | undefined = [];
const install = vi.fn();

vi.mock("../../features/update/updateStore", () => ({
  useUpdateState: () => state,
  installUpdate: () => install(),
}));
vi.mock("../../features/play/useGameSync", () => ({ useRunning: () => running }));

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  state = { kind: "idle" };
  running = [];
});

const openButton = () => screen.getByRole("button", { name: "A new version is available" });
const installButton = () => screen.getByRole("button", { name: "Update and restart" });

describe("UpdateButton", () => {
  it.each<UpdateState>([{ kind: "idle" }, { kind: "checking" }, { kind: "upToDate" }, { kind: "failed" }])(
    "renders nothing while $kind",
    (s) => {
      state = s;
      const { container } = render(<UpdateButton />);
      expect(container.innerHTML).toBe("");
    },
  );

  it("shows an icon button that opens the version and the install button", () => {
    state = { kind: "available", version: "0.1.1" };
    render(<UpdateButton />);
    fireEvent.click(openButton());
    expect(screen.getByText("0.1.1")).toBeTruthy();
    fireEvent.click(installButton());
    expect(install).toHaveBeenCalledTimes(1);
  });

  it("install is disabled while a game is running", () => {
    state = { kind: "available", version: "0.1.1" };
    running = [{ instanceId: "a" }];
    render(<UpdateButton />);
    fireEvent.click(openButton());
    expect(installButton().hasAttribute("disabled")).toBe(true);
  });

  it("install is disabled while it is unknown whether a game is running", () => {
    state = { kind: "available", version: "0.1.1" };
    running = undefined;
    render(<UpdateButton />);
    fireEvent.click(openButton());
    expect(installButton().hasAttribute("disabled")).toBe(true);
  });

  it("shows progress while downloading", () => {
    state = { kind: "downloading", percent: 40 };
    render(<UpdateButton />);
    fireEvent.click(openButton());
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("40");
    expect(screen.queryByRole("button", { name: "Update and restart" })).toBeNull();
  });
});

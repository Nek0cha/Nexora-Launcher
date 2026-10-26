import { afterEach, describe, expect, it } from "vitest";
import { dropAction, dropZoneAt, passedDragThreshold } from "./chipDrag";

function zone(html: string): HTMLElement {
  document.body.innerHTML = html;
  return document.getElementById("hit") as HTMLElement;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("dropZoneAt", () => {
  it("finds the profile card around the element under the pointer", () => {
    const hit = zone(`<section data-drop-zone="profile" data-profile-id="p1"><div><span id="hit"></span></div></section>`);
    expect(dropZoneAt(hit)).toEqual({ kind: "profile", profileId: "p1" });
  });

  it("finds the list of instances that have not joined", () => {
    const hit = zone(`<aside data-drop-zone="unjoined"><p id="hit"></p></aside>`);
    expect(dropZoneAt(hit)).toEqual({ kind: "unjoined" });
  });

  it("uses the nearest zone when zones are nested", () => {
    const hit = zone(
      `<div data-drop-zone="unjoined"><section data-drop-zone="profile" data-profile-id="p2"><b id="hit"></b></section></div>`,
    );
    expect(dropZoneAt(hit)).toEqual({ kind: "profile", profileId: "p2" });
  });

  it("is outside when no zone is around the element", () => {
    const hit = zone(`<main><p id="hit"></p></main>`);
    expect(dropZoneAt(hit)).toEqual({ kind: "outside" });
  });

  it("is outside for a profile zone without an id", () => {
    const hit = zone(`<section data-drop-zone="profile"><p id="hit"></p></section>`);
    expect(dropZoneAt(hit)).toEqual({ kind: "outside" });
  });

  it("is null when the pointer is not over the page", () => {
    expect(dropZoneAt(null)).toBeNull();
  });
});

describe("dropAction", () => {
  const unjoined = { instanceId: "i1", profileId: null };
  const member = { instanceId: "i2", profileId: "p1" };

  it("joins when a chip that has not joined is dropped on a card", () => {
    expect(dropAction(unjoined, { kind: "profile", profileId: "p1" })).toEqual({
      kind: "join",
      instanceId: "i1",
      profileId: "p1",
    });
  });

  it("moves a member to another card by joining it", () => {
    expect(dropAction(member, { kind: "profile", profileId: "p2" })).toEqual({
      kind: "join",
      instanceId: "i2",
      profileId: "p2",
    });
  });

  it("does nothing when a member is dropped back on its own card", () => {
    expect(dropAction(member, { kind: "profile", profileId: "p1" })).toEqual({ kind: "none" });
  });

  it("leaves when a member is dropped outside the cards", () => {
    expect(dropAction(member, { kind: "outside" })).toEqual({ kind: "leave", instanceId: "i2" });
    expect(dropAction(member, { kind: "unjoined" })).toEqual({ kind: "leave", instanceId: "i2" });
  });

  it("does nothing when a chip that has not joined is dropped outside the cards", () => {
    expect(dropAction(unjoined, { kind: "outside" })).toEqual({ kind: "none" });
    expect(dropAction(unjoined, { kind: "unjoined" })).toEqual({ kind: "none" });
  });

  it("does nothing when the pointer left the page", () => {
    expect(dropAction(member, null)).toEqual({ kind: "none" });
    expect(dropAction(unjoined, null)).toEqual({ kind: "none" });
  });
});

describe("passedDragThreshold", () => {
  it("starts a drag only after the pointer moved a few pixels", () => {
    expect(passedDragThreshold(0, 0)).toBe(false);
    expect(passedDragThreshold(2, 2)).toBe(false);
    expect(passedDragThreshold(4, 0)).toBe(true);
    expect(passedDragThreshold(-3, -3)).toBe(true);
  });
});

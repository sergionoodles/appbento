import assert from "node:assert/strict";
import { test } from "vitest";
import {
  EMPTY_LAYOUT, UNGROUPED_ID, arrangeLayout, createGroup, deleteGroup, dropIndex, moveApp, parseLayout,
  readLayout, renameGroup, shiftApp, shiftGroup, uniqueGroupName, writeLayout, type AppLayout,
} from "../src/lib/app-layout.ts";

const grouped: AppLayout = {
  ungrouped: ["a", "b"],
  groups: [
    { id: "dev", name: "Development", appIds: ["c", "d"], isCollapsed: false },
    { id: "video", name: "Video", appIds: ["e"], isCollapsed: false },
  ],
};

test("arranging places every installed app once and appends new apps to the ungrouped list", () => {
  const saved: AppLayout = { ungrouped: ["gone", "b"], groups: [{ id: "dev", name: "Dev", appIds: ["c", "b"], isCollapsed: false }] };
  assert.deepEqual(arrangeLayout(saved, ["a", "b", "c"]), {
    ungrouped: ["a"],
    groups: [{ id: "dev", name: "Dev", appIds: ["c", "b"], isCollapsed: false }],
  });
});

test("moving an app reorders within a section and across groups without mutating the input", () => {
  const snapshot = structuredClone(grouped);
  assert.deepEqual(moveApp(grouped, "a", UNGROUPED_ID, 1).ungrouped, ["b", "a"]);
  const moved = moveApp(grouped, "a", "video", 0);
  assert.deepEqual(moved.ungrouped, ["b"]);
  assert.deepEqual(moved.groups[1].appIds, ["a", "e"]);
  assert.deepEqual(moveApp(grouped, "a", "missing", 0), grouped);
  assert.deepEqual(grouped, snapshot);
});

test("shifting an app stays inside its section bounds", () => {
  assert.deepEqual(shiftApp(grouped, "d", -1).groups[0].appIds, ["d", "c"]);
  assert.deepEqual(shiftApp(grouped, "d", 1).groups[0].appIds, ["c", "d"]);
  assert.deepEqual(shiftApp(grouped, "a", -1).ungrouped, ["a", "b"]);
});

test("drop index is counted without the dragged app", () => {
  assert.equal(dropIndex(grouped, "a", UNGROUPED_ID, "b", true), 1);
  assert.equal(dropIndex(grouped, "a", "dev", "d", false), 1);
  assert.equal(dropIndex(grouped, "c", "dev", "d", true), 1);
  assert.equal(dropIndex(grouped, "a", "video", "unknown", false), 1);
});

test("groups can be created with apps, renamed, reordered and deleted without losing apps", () => {
  const created = createGroup(grouped, "audio", uniqueGroupName(grouped), ["b"]);
  assert.deepEqual(created.ungrouped, ["a"]);
  assert.deepEqual(created.groups[2], { id: "audio", name: "New group", appIds: ["b"], isCollapsed: false });
  assert.equal(uniqueGroupName(created), "New group 2");
  assert.equal(renameGroup(created, "audio", "  Audio ").groups[2].name, "Audio");
  assert.equal(renameGroup(created, "audio", "   ").groups[2].name, "New group");
  assert.deepEqual(shiftGroup(created, "audio", -1).groups.map((group) => group.id), ["dev", "audio", "video"]);
  assert.deepEqual(shiftGroup(created, "dev", -1), created);
  assert.deepEqual(deleteGroup(created, "dev"), { ungrouped: ["a", "c", "d"], groups: [created.groups[1], created.groups[2]] });
});

test("saved layouts are validated and storage failures are reported", () => {
  assert.deepEqual(parseLayout(null), EMPTY_LAYOUT);
  assert.deepEqual(parseLayout('{"ungrouped":["a",1],"groups":[{"id":"x","name":"X","appIds":["b"]},{"id":2}]}'), {
    ungrouped: [],
    groups: [{ id: "x", name: "X", appIds: ["b"], isCollapsed: false }],
  });
  const corrupt = () => ({ getItem: () => "{not json" }) as unknown as Storage;
  assert.deepEqual(readLayout(corrupt), { layout: EMPTY_LAYOUT, error: "The saved app arrangement was unreadable and has been reset." });
  const blocked = (): Storage => { throw new Error("Storage blocked"); };
  assert.equal(writeLayout(grouped, blocked), "The app arrangement cannot be saved in this session.");
});

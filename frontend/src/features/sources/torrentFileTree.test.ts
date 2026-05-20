import { describe, expect, it } from "vitest";
import { buildTorrentFileTree, flattenTorrentTree } from "./torrentFileTree";

describe("torrentFileTree", () => {
  it("builds nested folders from paths", () => {
    const tree = buildTorrentFileTree([
      { index: 0, path: "Album/track1.mp3", size_bytes: 100, selected: true },
      { index: 1, path: "Album/track2.mp3", size_bytes: 200, selected: true },
      { index: 2, path: "cover.jpg", size_bytes: 50, selected: true },
    ]);
    expect(tree.map((n) => n.name)).toEqual(["Album", "cover.jpg"]);
    const flat = flattenTorrentTree(tree);
    expect(flat.filter((n) => n.kind === "file")).toHaveLength(3);
  });
});

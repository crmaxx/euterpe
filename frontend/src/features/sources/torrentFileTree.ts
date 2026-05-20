import type { TorrentInspectFile } from "@/api/client";

export type TorrentTreeNode = {
  id: string;
  name: string;
  kind: "folder" | "file";
  depth: number;
  children: TorrentTreeNode[];
  file?: TorrentInspectFile;
};

export function buildTorrentFileTree(files: TorrentInspectFile[]): TorrentTreeNode[] {
  const root: TorrentTreeNode = {
    id: "",
    name: "",
    kind: "folder",
    depth: -1,
    children: [],
  };

  for (const file of files) {
    const parts = file.path.split(/[/\\]/).filter((p) => p.length > 0);
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLeaf = i === parts.length - 1;
      const id = parts.slice(0, i + 1).join("/");

      if (isLeaf) {
        current.children.push({
          id,
          name: part,
          kind: "file",
          depth: i,
          children: [],
          file,
        });
      } else {
        let next = current.children.find(
          (c) => c.kind === "folder" && c.name === part,
        );
        if (!next) {
          next = {
            id,
            name: part,
            kind: "folder",
            depth: i,
            children: [],
          };
          current.children.push(next);
        }
        current = next;
      }
    }
  }

  sortTreeNodes(root.children);
  return root.children;
}

function sortTreeNodes(nodes: TorrentTreeNode[]) {
  nodes.sort((a, b) => {
    if (a.kind !== b.kind) {
      return a.kind === "folder" ? -1 : 1;
    }
    return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  });
  for (const n of nodes) {
    if (n.kind === "folder") {
      sortTreeNodes(n.children);
    }
  }
}

export function flattenTorrentTree(nodes: TorrentTreeNode[]): TorrentTreeNode[] {
  const out: TorrentTreeNode[] = [];
  const walk = (list: TorrentTreeNode[]) => {
    for (const n of list) {
      out.push(n);
      if (n.kind === "folder" && n.children.length > 0) {
        walk(n.children);
      }
    }
  };
  walk(nodes);
  return out;
}

export function filterTorrentFiles(
  files: TorrentInspectFile[],
  query: string,
): TorrentInspectFile[] {
  const q = query.trim().toLowerCase();
  if (!q) return files;
  return files.filter((f) => f.path.toLowerCase().includes(q));
}

export function collectFileIndices(node: TorrentTreeNode): number[] {
  if (node.kind === "file" && node.file) {
    return [node.file.index];
  }
  return node.children.flatMap(collectFileIndices);
}

export function folderSelectionState(
  node: TorrentTreeNode,
  selection: Record<number, boolean>,
): "all" | "some" | "none" {
  const indices = collectFileIndices(node);
  if (indices.length === 0) return "none";
  const selected = indices.filter((i) => selection[i]).length;
  if (selected === 0) return "none";
  if (selected === indices.length) return "all";
  return "some";
}

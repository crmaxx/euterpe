import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type {
  TorrentInspectResponse,
  TorrentPostDownloadOptions,
} from "@/api/client";
import { TestProviders } from "@/test/test-providers";
import { TorrentInspectView } from "./TorrentInspectView";

function makeInspect(format: "flac" | "ape"): TorrentInspectResponse {
  return {
    inspect_id: "inspect-1",
    name: "Image + CUE",
    total_size_bytes: 1024,
    info_hash_v1: "abcdef123456",
    info_hash_v2: null,
    comment: null,
    free_space_bytes: null,
    files: [
      { index: 0, path: "Album/image.cue", size_bytes: 100, selected: true },
      {
        index: 1,
        path: `Album/image.${format}`,
        size_bytes: 924,
        selected: true,
      },
    ],
    post_download_capability: {
      has_flac_image_cue: format === "flac",
      has_convertible_image_cue: format !== "flac",
      cue_candidates: [
        {
          cue_path: "Album/image.cue",
          audio_path: `Album/image.${format}`,
          audio_format: format,
          direct_split_supported: format === "flac",
          convert_required_for_split: format !== "flac",
        },
      ],
    },
  };
}

function Harness({ inspect }: { inspect: TorrentInspectResponse }) {
  const [selection, setSelection] = useState<Record<number, boolean>>({
    0: true,
    1: true,
  });
  const [postDownload, setPostDownload] =
    useState<TorrentPostDownloadOptions | null>({
      convert_after_download: false,
      split_after_download: false,
      split_after_conversion: false,
      cue_path: "Album/image.cue",
      source_file_policy: "keep",
    });

  return (
    <TestProviders>
      <TorrentInspectView
        inspect={inspect}
        selection={selection}
        copyToLibrary={true}
        autoIndex={true}
        postDownload={postDownload}
        busy={false}
        onSelectionChange={setSelection}
        onCopyToLibraryChange={vi.fn()}
        onAutoIndexChange={vi.fn()}
        onPostDownloadChange={setPostDownload}
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />
    </TestProviders>
  );
}

describe("TorrentInspectView CUE post-download controls", () => {
  it("allows direct split for a selected FLAC image and CUE", () => {
    render(<Harness inspect={makeInspect("flac")} />);

    expect(screen.getByRole("checkbox", { name: /split after download/i }))
      .toBeEnabled();
    expect(
      screen.queryByRole("checkbox", { name: /convert after download/i }),
    ).not.toBeInTheDocument();
  });

  it("requires conversion before split for a non-FLAC image and CUE", async () => {
    const user = userEvent.setup();
    render(<Harness inspect={makeInspect("ape")} />);

    const convert = screen.getByRole("checkbox", {
      name: /convert after download/i,
    });
    const split = screen.getByRole("checkbox", {
      name: /split after conversion/i,
    });

    expect(convert).toBeEnabled();
    expect(split).toBeDisabled();

    await user.click(convert);

    expect(split).toBeEnabled();
  });
});

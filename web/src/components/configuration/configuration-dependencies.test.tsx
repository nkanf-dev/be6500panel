import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Schema } from "effect";
import { DraftSchema } from "./contracts";
import { DraftDiff } from "./DraftQueue";

describe("dependent configuration drafts", () => {
  it("keeps native-valid dependent draft selectable but does not claim references resolved", () => {
    const draft = Schema.decodeUnknownSync(DraftSchema)({
      id: "a".repeat(32),
      module: "dhcp",
      generation: 2,
      diff: "",
      risks: [],
      valid: true,
      errors: [],
      dependencies: [
        {
          code: "invalid_reference",
          message: "Unknown dhcp configuration reference: interface.",
        },
      ],
      createdAt: "2026-10-03T00:00:00Z",
    });
    expect(draft.valid).toBe(true);
    expect(draft.dependencies).toHaveLength(1);
    render(<DraftDiff draft={draft} />);
    expect(screen.getByText("引用待完整检查")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("一并选择所需草稿");
    expect(screen.queryByText("检查通过")).not.toBeInTheDocument();
  });
});

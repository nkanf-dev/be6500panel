import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NativeFields } from "./NativeFields";
import { nativeSections } from "./native-document";
import { fieldSchema } from "./field-schema";

function renderFields(module: "dropbear" | "wireless", content: string) {
  const change = vi.fn();
  render(
    <NativeFields
      module={module}
      section={nativeSections(content)[0]}
      content={content}
      onChange={change}
    />,
  );
  return change;
}

describe("expandable field help", () => {
  it("keeps detailed explanations collapsed and preserves native input bounds", () => {
    const change = renderFields(
      "dropbear",
      "config dropbear 'main'\n option Port '2200'\n",
    );
    const help = fieldSchema("dropbear", "dropbear", "Port").help!;
    expect(screen.getByText(help.description)).not.toBeVisible();
    fireEvent.click(screen.getByText("SSH 端口使用说明"));
    expect(screen.getByText(help.description)).toBeVisible();
    expect(screen.getByText(help.impact)).toBeVisible();
    expect(screen.getByText(/1.0.43 静态固件/)).not.toBeVisible();
    fireEvent.click(screen.getByText("固件来源与版本"));
    expect(screen.getByText(/1.0.43 静态固件/)).toBeVisible();
    expect(screen.getAllByText("Xiaomi RN02 1.0.43").length).toBeGreaterThan(0);
    expect(screen.getByLabelText("SSH 端口 (Port)")).toHaveAttribute(
      "min",
      "1",
    );
    expect(screen.getByLabelText("SSH 端口 (Port)")).toHaveAttribute(
      "max",
      "65535",
    );
    expect(change).not.toHaveBeenCalled();
  });

  it("does not project a credential value into any explanatory help", () => {
    const secret = "synthetic-secret-for-ui-test";
    renderFields(
      "wireless",
      `config wifi-iface 'main'\n option key '${secret}'\n`,
    );
    fireEvent.click(screen.getByText("无线密码 / 密钥使用说明"));
    expect(screen.getByLabelText("无线密码 / 密钥 (key)")).toHaveAttribute(
      "type",
      "password",
    );
    expect(screen.getByLabelText("无线密码 / 密钥 (key)")).toHaveValue(secret);
    const explanation = screen
      .getByText("无线密码 / 密钥使用说明")
      .closest("details")!;
    expect(explanation.textContent).not.toContain(secret);
    expect(screen.getByText(/凭据/)).toBeVisible();
  });

  it("keeps unregistered vendor fields editable without invented firmware help", () => {
    const change = renderFields(
      "wireless",
      "config wifi-iface 'main'\n option vendor_mode 'raw-token'\n",
    );
    const input = screen.getByLabelText("vendor_mode (vendor_mode)");
    expect(input).toHaveValue("raw-token");
    expect(input).toHaveAttribute("type", "text");
    expect(screen.queryByText("vendor_mode使用说明")).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "updated-token" } });
    expect(change).toHaveBeenCalledWith(
      "config wifi-iface 'main'\n option vendor_mode 'updated-token'\n",
    );
  });

  it("preserves unknown select tokens and non-scalar numeric values", () => {
    const change = renderFields(
      "wireless",
      "config wifi-device 'radio0'\n option band 'vendor-5'\n option txpower 'auto'\n",
    );
    fireEvent.click(screen.getByText("频段使用说明"));
    fireEvent.click(screen.getByText("发射功率使用说明"));
    expect(screen.getByLabelText("频段 (band)")).toHaveValue("vendor-5");
    expect(
      screen.getByRole("option", { name: "vendor-5 · 当前值" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("发射功率 (txpower)")).toHaveAttribute(
      "type",
      "text",
    );
    expect(screen.getByLabelText("发射功率 (txpower)")).toHaveValue("auto");
    expect(change).not.toHaveBeenCalled();
  });
});

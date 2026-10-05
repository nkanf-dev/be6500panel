import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { FeatureFieldInput } from "./feature-field-input";
import { FeatureActionForm } from "./feature-action-form";
import { FeatureDomainPanel } from "./feature-domain-panel";
import { StateDataViewer } from "./state-data-viewer";
import type { FeatureAction, FeatureDomain, FeatureField, FeatureState } from "../../lib/features-api";
import * as featuresApiModule from "../../lib/features-api";
import { Effect } from "effect";
import {ApiError} from "../../lib/api";

afterEach(() => vi.restoreAllMocks());

describe("FeatureFieldInput", () => {
  it("renders text input and triggers onChange", () => {
    const field: FeatureField = { key: "ssid", label: "无线名称", kind: "text", required: true };
    const onChange = vi.fn();
    render(<FeatureFieldInput field={field} value="MyHome" onChange={onChange} />);

    const input = screen.getByLabelText("无线名称");
    expect(input).toHaveValue("MyHome");
    fireEvent.change(input, { target: { value: "NewHome" } });
    expect(onChange).toHaveBeenCalledWith("NewHome");
  });

  it("renders secret input with configured placeholder and toggle eye", () => {
    const field: FeatureField = { key: "key", label: "Wi-Fi 密码", kind: "secret", required: false };
    const onChange = vi.fn();
    render(<FeatureFieldInput field={field} value="" configured={true} onChange={onChange} />);

    const input = screen.getByLabelText("Wi-Fi 密码");
    expect(input).toHaveAttribute("type", "password");
    expect(input).toHaveAttribute("placeholder", "已配置（留空保留）");
    expect(screen.getByText("已保留")).toBeInTheDocument();

    const toggleBtn = screen.getByRole("button", { name: "显示密码" });
    fireEvent.click(toggleBtn);
    expect(input).toHaveAttribute("type", "text");
  });
});

  it("renders boolean input with native '0' string as unchecked and accessible by label", () => {
    const field: FeatureField = { key: "guest_enabled", label: "访客网络开关", kind: "boolean", required: false };
    const onChange = vi.fn();
    render(<FeatureFieldInput field={field} value="0" onChange={onChange} />);

    const checkbox = screen.getByLabelText("访客网络开关");
    expect(checkbox).not.toBeChecked();

    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenCalledWith(true);
  });

describe("StateDataViewer", () => {
  it("renders table for list entries and calls onSelectTarget when clicked", () => {
    const data = {
      rules: [
        { name: "SSH Forward", srcport: 22, destip: "192.168.31.50" },
        { name: "Web Forward", srcport: 80, destip: "192.168.31.51" },
      ],
      enabled: true,
      guest_wifi: "0",
      protocol: "1",
    };
    const onSelect = vi.fn();
    render(<StateDataViewer data={data} onSelectTarget={onSelect} />);

    expect(screen.getByText("SSH Forward")).toBeInTheDocument();
    expect(screen.getByText("192.168.31.50")).toBeInTheDocument();
    expect(screen.getByText("已关闭")).toBeInTheDocument();
    // Numeric protocol '1' must remain '1', not falsely rendered as '已启用'
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getAllByText("外部端口")).toHaveLength(1);

    const buttons = screen.getAllByRole("button", { name: /选择编辑/ });
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]);
    expect(onSelect).toHaveBeenCalledWith(data.rules[0]);
  });
});

describe("FeatureActionForm", () => {
  const sampleAction: FeatureAction = {
    id: "set_wifi",
    title: "修改 Wi-Fi 设置",
    fields: [
      { key: "ssid", label: "无线名称", kind: "text", required: true },
      { key: "password", label: "密码", kind: "secret", required: false },
    ],
    impact: "wireless",
  };

  const sampleState: FeatureState = {
    available: true,
    readId: "wifi_info",
    generation: 15,
    data: {
      ssid: "Home_5G",
      passwordConfigured: true,
    },
  };

  it("omits empty secret fields to preserve existing backend secrets and passes generation", async () => {
    const applyMock = vi.spyOn(featuresApiModule.featuresApi, "apply").mockReturnValue(
      Effect.succeed({
        operation: {
          id: "op-1",
          state: "completed",
          actionId: "set_wifi",
          domain: "wireless",
          generation: 16,
        },
      }),
    );

    render(
      <FeatureActionForm
        domain="wireless"
        action={sampleAction}
        state={sampleState}
        catalogGeneration={15}
      />,
    );

    const ssidInput = screen.getByLabelText("无线名称");
    fireEvent.change(ssidInput, { target: { value: "Home_WiFi6" } });

    const submitBtn = screen.getByRole("button", { name: /应用设置/ });
    fireEvent.click(submitBtn);

    expect(screen.getByText("确认执行操作？")).toBeInTheDocument();
    const confirmBtn = screen.getByRole("button", { name: "确认继续" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(applyMock).toHaveBeenCalledTimes(1);
    });

    const callArgs = applyMock.mock.calls[0];
    expect(callArgs[0]).toBe("wireless");
    expect(callArgs[1].actionId).toBe("set_wifi");
    expect(callArgs[1].generation).toBe(15);
    expect(callArgs[1].acknowledgeImpact).toBe(true);
    expect(callArgs[1].input).toEqual({ ssid: "Home_WiFi6" });
    expect("password" in callArgs[1].input).toBe(false);
  });

  it("renders connection confirm banner and handles confirm action when canConfirm is true", async () => {
    const lanAction: FeatureAction = {
      id: "set_lan",
      title: "修改局域网 IP",
      fields: [{ key: "ip", label: "内网 IP", kind: "ipv4", required: true }],
      impact: "network",
    };

    const applyMock = vi.spyOn(featuresApiModule.featuresApi, "apply").mockReturnValue(
      Effect.succeed({
        operation: {
          id: "op-lan-100",
          state: "pending",
          actionId: "set_lan",
          domain: "network",
          generation: 20,
          canConfirm: true,
          reconnectAddress: "192.168.50.1",
          waitingFor: "等待新地址连通确认",
        },
      }),
    );

    const confirmMock = vi.spyOn(featuresApiModule.featuresApi, "confirm").mockReturnValue(
      Effect.succeed({
        id: "op-lan-100",
        state: "completed",
        actionId: "set_lan",
        domain: "network",
        generation: 21,
      }),
    );

    render(
      <FeatureActionForm
        domain="network"
        action={lanAction}
        state={{ available: true, readId: "lan", generation: 20, data: { ip: "192.168.31.1" } }}
      />,
    );

    const ipInput = screen.getByLabelText("内网 IP");
    fireEvent.change(ipInput, { target: { value: "192.168.50.1" } });

    const submitBtn = screen.getByRole("button", { name: /应用设置/ });
    fireEvent.click(submitBtn);

    // Network impact confirmation dialog
    expect(screen.getByText("确认执行操作？")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认继续" }));

    await waitFor(() => {
      expect(applyMock).toHaveBeenCalledTimes(1);
    });

    // Confirmation banner should be displayed with reconnectAddress and single button
    expect(screen.getByText("配置已应用，等待连接确认")).toBeInTheDocument();
    expect(screen.getByText("等待新地址连通确认")).toBeInTheDocument();
    expect(screen.getByText(/192\.168\.50\.1/)).toBeInTheDocument();

    const confirmBtn = screen.getByRole("button", { name: "确认连接正常" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(confirmMock).toHaveBeenCalledWith("op-lan-100");
    });
    expect(screen.getByText("连接正常，配置已确认生效")).toBeInTheDocument();
  });
});

describe("FeatureDomainPanel", () => {
  const domain: FeatureDomain = {
    id: "network",
    title: "网络配置",
    reads: [
      {
        id: "wan",
        title: "WAN 设置",
        fields: [],
      },
    ],
    actions: [
      {
        id: "save_wan",
        title: "保存 WAN",
        fields: [{ key: "proto", label: "协议", kind: "text", required: true }],
        impact: "network",
        readback: "wan",
      },
    ],
  };

  it("shows 已读取 badge and displays projected list data", async () => {
    vi.spyOn(featuresApiModule.featuresApi, "state").mockReturnValue(
      Effect.succeed({
        available: true,
        readId: "wan",
        generation: 5,
        data: { proto: "dhcp" },
      }),
    );

    render(<FeatureDomainPanel domain={domain} catalogGeneration={5} />);

    await waitFor(() => {
      expect(screen.getByText("已读取")).toBeInTheDocument();
    });
    expect(screen.getByText("dhcp")).toBeInTheDocument();
    expect(screen.getByText("保存 WAN")).toBeInTheDocument();
  });

  it("discards aborted request when a newer request succeeds (synthetic race test)", async () => {
    let callIndex = 0;
    vi.spyOn(featuresApiModule.featuresApi, "state").mockImplementation(() => {
      callIndex++;
      if (callIndex === 1) {
        // Return a slow deferred effect
        return Effect.async<FeatureState, any>((resume) => {
          setTimeout(() => {
            resume(Effect.fail(new ApiError({ code: "aborted", message: "An error has occurred" })));
          }, 50);
        });
      }
      return Effect.succeed({
        available: true,
        readId: "wan",
        generation: 6,
        data: { proto: "static_ip" },
      });
    });

    const { rerender } = render(<FeatureDomainPanel domain={domain} catalogGeneration={5} />);
    const refreshedDomain = { ...domain, reads: domain.reads.map(read => ({ ...read })) };
    rerender(<FeatureDomainPanel domain={refreshedDomain} catalogGeneration={6} />);

    await waitFor(() => {
      expect(screen.getByText("已读取")).toBeInTheDocument();
      expect(screen.getByText("static_ip")).toBeInTheDocument();
    });

    expect(screen.queryByText(/An error has occurred/)).not.toBeInTheDocument();
    expect(screen.queryByText("读取失败")).not.toBeInTheDocument();
  });

  it("retains last state data but displays error banner and failure badge when refresh fails with 503", async () => {
    let callCount = 0;
    vi.spyOn(featuresApiModule.featuresApi, "state").mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        return Effect.succeed({
          available: true,
          readId: "wan",
          generation: 7,
          data: { proto: "dhcp_active" },
        });
      }
      return Effect.fail(
        new ApiError({
          status: 503,
          code: "service_unavailable",
          message: "原厂服务暂不可用",
        }),
      );
    });

    render(<FeatureDomainPanel domain={domain} catalogGeneration={7} />);

    await waitFor(() => {
      expect(screen.getByText("已读取")).toBeInTheDocument();
      expect(screen.getByText("dhcp_active")).toBeInTheDocument();
    });

    // Click refresh
    const refreshBtn = screen.getByRole("button", { name: "刷新" });
    fireEvent.click(refreshBtn);

    await waitFor(() => {
      expect(screen.getByText("读取失败 · 上次数据")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("原厂服务暂不可用");
      expect(screen.getByRole("alert")).toHaveTextContent("当前保留上次成功读取的记录");
      // Verify previous data is still retained on screen
      expect(screen.getByText("dhcp_active")).toBeInTheDocument();
    });
  });

  it("displays 未配置 badge and does not pre-fill credentials when state data has configured: false", async () => {
    const ddnsDomain: FeatureDomain = {
      id: "services",
      title: "服务管理",
      reads: [
        {
          id: "ddns_detail",
          title: "DDNS 实例设置",
          fields: [{ key: "id", label: "实例 ID", kind: "integer", required: true, min: 1, max: 9 }],
        },
      ],
      actions: [
        {
          id: "ddns_edit",
          title: "修改 DDNS 实例",
          fields: [
            { key: "username", label: "用户名", kind: "text", required: false },
            { key: "password", label: "密码", kind: "secret", required: false },
          ],
          impact: "local",
          readback: "ddns_detail",
        },
      ],
    };

    vi.spyOn(featuresApiModule.featuresApi, "state").mockReturnValue(
      Effect.succeed({
        available: true,
        readId: "ddns_detail",
        generation: 1,
        data: {
          configured: false,
          id: 1,
        },
      }),
    );

    render(<FeatureDomainPanel domain={ddnsDomain} catalogGeneration={1} />);

    const idInput = screen.getByLabelText("实例 ID");
    fireEvent.change(idInput, { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "读取数据" }));

    await waitFor(() => {
      expect(screen.getByText("未配置", { selector: "span" })).toBeInTheDocument();
      expect(screen.getByText("未配置", { selector: "dd" })).toBeInTheDocument();
    });

    const pwdInput = screen.getByLabelText("密码");
    expect(pwdInput).toHaveAttribute("placeholder", "请输入密码或密钥");
    expect(screen.queryByText("已保留")).not.toBeInTheDocument();
  });
});



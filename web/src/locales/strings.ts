/** Canonical user-facing actions. UX/copy review owns this vocabulary. */
export const strings = {
  actions: {
    applyChanges: "应用更改",
    saveDraft: "保存草稿",
    restorePrevious: "恢复上一配置",
    confirmWorking: "确认生效",
    cancel: "取消",
    refresh: "刷新状态",
    confirmCapture: "确认接管",
  },
  states: {
    draft: "草稿",
    active: "已生效",
    restoreFailed: "恢复失败",
    pending: (seconds: number) => `待确认生效 (${seconds}s)`,
  },
  dialogs: {
    applyRisk: "确认应用高风险更改",
    capture: "开启客户端透明接管",
    captureBody: (devices: string) => `即将为设备 [${devices}] 开启透明代理转发，请确认是否继续。`,
  },
} as const;

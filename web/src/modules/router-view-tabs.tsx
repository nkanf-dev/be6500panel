export type RouterView = "observation" | "features" | "configuration";

export function RouterViewTabs({
  label,
  value,
  onChange,
  featuresLabel = "功能设置",
  showFeatures = true,
}: {
  label: string;
  value: RouterView;
  onChange: (view: RouterView) => void;
  featuresLabel?: string;
  showFeatures?: boolean;
}) {
  const tabs = [
    { value: "observation" as const, label: "观察" },
    ...(showFeatures ? [{ value: "features" as const, label: featuresLabel }] : []),
    { value: "configuration" as const, label: "配置" },
  ];

  return (
    <div className="segmented" role="tablist" aria-label={label}>
      {tabs.map((tab) => (
        <button
          key={tab.value}
          type="button"
          role="tab"
          aria-selected={value === tab.value}
          onClick={() => onChange(tab.value)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

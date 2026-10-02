export type RouterView = "observation" | "configuration";

export function RouterViewTabs({
  label,
  value,
  onChange,
}: {
  label: string;
  value: RouterView;
  onChange: (view: RouterView) => void;
}) {
  return (
    <div className="segmented" role="tablist" aria-label={label}>
      {(
        [
          { value: "observation", label: "观察" },
          { value: "configuration", label: "配置" },
        ] as const
      ).map((tab) => (
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

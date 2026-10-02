import type { ConfigurationModule } from "./contracts";
import { ConfigurationSurface } from "./ConfigurationSurface";
import { NativeEditor } from "./NativeEditor";
import { useConfiguration } from "./use-configuration";
import "./configuration.css";

/** Authenticated native document editor. Stage never applies live settings. */
export function ConfigurationEditor({
  module,
}: {
  module: ConfigurationModule;
}) {
  const controller = useConfiguration();
  return (
    <ConfigurationSurface title="配置编辑" controller={controller}>
      <NativeEditor key={module} module={module} controller={controller} />
    </ConfigurationSurface>
  );
}

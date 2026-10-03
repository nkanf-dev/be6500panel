import { api } from "../../lib/api";
import type { ProxySelectInput } from "../../lib/contracts";

/** Keep the shared API's decoding, request limits, errors, and spies intact. */
export function selectProxyPolicy(
  body: ProxySelectInput & { acknowledgedRevision?: string },
) {
  return api.proxySelect(body);
}

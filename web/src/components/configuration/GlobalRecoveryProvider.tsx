import { createContext, useContext, type ReactNode } from "react";
import {
  useGlobalRecoveryState,
  type GlobalRecoveryController,
} from "./use-global-recovery";

const GlobalRecoveryContext = createContext<
  GlobalRecoveryController | undefined
>(undefined);

/** Mount once under the authenticated console, outside all page/branch content. */
export function GlobalRecoveryProvider({ children }: { children: ReactNode }) {
  const controller = useGlobalRecoveryState();
  return (
    <GlobalRecoveryContext.Provider value={controller}>
      {children}
    </GlobalRecoveryContext.Provider>
  );
}

export function useGlobalRecovery() {
  const controller = useContext(GlobalRecoveryContext);
  if (!controller)
    throw new Error("useGlobalRecovery requires GlobalRecoveryProvider");
  return controller;
}

import { useCallback, useEffect, useState } from "react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { api, errorMessage, runRequest } from "../lib/api";
import type { Session } from "../lib/contracts";
import { Button, ErrorState, Loading } from "../components/ui/primitives";
import { pageFromHash, type PageId } from "../modules/registry";
import { ConsoleProvider } from "./console-context";
import { Login } from "./login";
import { Shell } from "./shell";
import { ModulePage } from "./pages";
import { clearConfigurationSession } from "../components/configuration";
import { clearRuntimeEditorSession } from "../modules/runtime/editor-session";
import { GlobalRecoveryProvider } from "../components/configuration/GlobalRecoveryProvider";
import { GlobalRecoveryBanner } from "../components/configuration/GlobalRecoveryBanner";
import { DeviceLabelsProvider } from "../modules/devices";
import { clearFrpcFormSession } from "../modules/frpc-session";

export function App() {
  const [session, setSession] = useState<Session>();
  const [error, setError] = useState<unknown>();
  const [page, setPage] = useState<PageId>(pageFromHash);
  const checkSession = useCallback(() => {
    setError(undefined);
    runRequest(api.session()).then(setSession).catch(setError);
  }, []);
  useEffect(checkSession, [checkSession]);
  useEffect(() => {
    const listener = () => setPage(pageFromHash());
    window.addEventListener("hashchange", listener);
    return () => window.removeEventListener("hashchange", listener);
  }, []);
  const navigate = useCallback((id: PageId) => {
    window.location.hash = `/${id}`;
    setPage(id);
    window.scrollTo?.({ top: 0 });
  }, []);
  const onUnauthorized = useCallback(() => {
    clearConfigurationSession();
    clearRuntimeEditorSession();
    clearFrpcFormSession();
    setSession((previous) => ({
      authenticated: false,
      authRequired: previous?.authRequired ?? true,
    }));
  }, []);
  useEffect(() => {
    window.addEventListener("be6500panel:unauthorized", onUnauthorized);
    return () =>
      window.removeEventListener("be6500panel:unauthorized", onUnauthorized);
  }, [onUnauthorized]);
  const logout = async () => {
    try {
      const nextSession = await runRequest(api.logout());
      window.dispatchEvent(new Event("be6500panel:logout"));
      clearConfigurationSession();
      clearRuntimeEditorSession();
      clearFrpcFormSession();
      setSession(nextSession);
    } catch (error) {
      setError(error);
    }
  };
  return (
    <Tooltip.Provider delayDuration={300}>
      {!session ? (
        <div className="startup-state">
          <strong>be6500panel</strong>
          {error !== undefined ? (
            <>
              <ErrorState message={errorMessage(error)} />
              <Button onClick={checkSession}>重新连接</Button>
            </>
          ) : (
            <Loading label="连接控制平面…" />
          )}
        </div>
      ) : session.authRequired && !session.authenticated ? (
        <Login onLogin={setSession} />
      ) : (
        <ConsoleProvider onUnauthorized={onUnauthorized}>
          <GlobalRecoveryProvider>
            <DeviceLabelsProvider>
              <Shell
                page={page}
                navigate={navigate}
                authRequired={session.authRequired}
                onLogout={logout}
                recoveryBanner={<GlobalRecoveryBanner />}
              >
                <ModulePage page={page} navigate={navigate} />
              </Shell>
            </DeviceLabelsProvider>
          </GlobalRecoveryProvider>
        </ConsoleProvider>
      )}
    </Tooltip.Provider>
  );
}

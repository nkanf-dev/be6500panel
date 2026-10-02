import { useState, type FormEvent } from "react";
import { ArrowRight, LockKeyhole, Router } from "lucide-react";
import { api, errorMessage, runRequest } from "../lib/api";
import type { Session } from "../lib/contracts";
import { Button, ErrorState, Field } from "../components/ui/primitives";
import { ThemeMenu } from "./theme-menu";
export function Login({ onLogin }: { onLogin: (session: Session) => void }) {
  const [password, setPassword] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(undefined);
    try {
      onLogin(await runRequest(api.login(password)));
      setPassword("");
    } catch (error) {
      setError(error);
    } finally {
      setPending(false);
    }
  }
  return (
    <main className="login-layout">
      <div className="login-top">
        <span className="brand">
          <span className="brand-icon">
            <Router size={20} />
          </span>
          be6500panel
        </span>
        <ThemeMenu />
      </div>
      <section className="login-panel">
        <div className="login-symbol">
          <LockKeyhole size={24} />
        </div>
        <span className="eyebrow">LOCAL CONTROL PLANE</span>
        <h1>登录控制台</h1>
        <p>使用服务端设置的访问密码</p>
        <form onSubmit={submit}>
          <Field label="访问密码">
            <input
              type="password"
              autoFocus
              autoComplete="current-password"
              required
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="输入访问密码"
            />
          </Field>
          {error !== undefined && <ErrorState message={errorMessage(error)} />}
          <Button variant="primary" type="submit" disabled={pending}>
            {pending ? "正在验证…" : "登录"}
            <ArrowRight size={16} />
          </Button>
        </form>
        <div className="login-meta">
          <LockKeyhole size={12} />
          同源会话 · HttpOnly cookie
        </div>
      </section>
      <footer className="login-footer">
        be6500panel · 模块化路由器控制平面
      </footer>
    </main>
  );
}

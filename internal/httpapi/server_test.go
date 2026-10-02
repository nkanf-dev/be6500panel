package httpapi

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"be6500panel/internal/core"
	"be6500panel/internal/modules"
)

func testServer(t *testing.T, password string) (*Server, *httptest.Server) {
	t.Helper()
	observer := modules.NewSystem(true)
	sampler := core.NewSampler(observer.Observe, time.Hour)
	ctx, cancel := context.WithCancel(context.Background())
	sampler.Start(ctx)
	srv, err := New(Config{System: observer, Network: modules.Network{}, Sampler: sampler, Password: password, Heartbeat: 10 * time.Millisecond})
	if err != nil {
		t.Fatal(err)
	}
	ts := httptest.NewServer(srv)
	t.Cleanup(func() { cancel(); srv.Close(); ts.Close() })
	return srv, ts
}
func request(t *testing.T, ts *httptest.Server, method, path, body string, cookie *http.Cookie) *http.Response {
	t.Helper()
	req, err := http.NewRequest(method, ts.URL+path, strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	if cookie != nil {
		req.AddCookie(cookie)
	}
	res, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { res.Body.Close() })
	return res
}
func decode(t *testing.T, r *http.Response) map[string]any {
	t.Helper()
	var v map[string]any
	if err := json.NewDecoder(r.Body).Decode(&v); err != nil {
		t.Fatal(err)
	}
	return v
}
func TestReadAPIs(t *testing.T) {
	_, ts := testServer(t, "")
	for _, path := range []string{"/api/health", "/api/modules", "/api/system", "/api/network", "/api/devices", "/api/frpc", "/api/session"} {
		t.Run(path, func(t *testing.T) {
			r := request(t, ts, "GET", path, "", nil)
			if r.StatusCode != 200 {
				t.Fatal(r.Status)
			}
			v := decode(t, r)
			switch path {
			case "/api/health":
				if v["mode"] != "demo" || v["readOnly"] != true {
					t.Fatal(v)
				}
			case "/api/system":
				if v["hostname"] != "be6500panel-demo" || v["sampledAt"] == nil {
					t.Fatal(v)
				}
			case "/api/network":
				if v["routeObservationSupported"] != false || v["interfaces"] == nil || v["routes"] == nil {
					t.Fatal(v)
				}
			case "/api/devices", "/api/frpc":
				if v["supported"] != false || v["reason"] == "" {
					t.Fatal(v)
				}
			case "/api/modules":
				if len(v["modules"].([]any)) != 8 {
					t.Fatal(v)
				}
			}
		})
	}
	for _, path := range []string{"/api/missing", "/api/system/", "/api", "/api/health/child"} {
		r := request(t, ts, "GET", path, "", nil)
		if r.StatusCode != 404 || decode(t, r)["error"] == nil {
			t.Fatal(r.Status)
		}
	}
	r := request(t, ts, "DELETE", "/api/system", "", nil)
	if r.StatusCode != 405 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "POST", "/api/operations/apply", "", nil)
	if r.StatusCode != 501 || decode(t, r)["error"].(map[string]any)["code"] != "not_implemented" {
		t.Fatal(r.Status)
	}
}

const validProxy = `{"mode":"split","dnsStrategy":"split","ipv6Policy":"follow","failurePolicy":"direct","nodeCount":2}`

func TestProxyValidation(t *testing.T) {
	_, ts := testServer(t, "")
	r := request(t, ts, "POST", "/api/proxy/plan", validProxy, nil)
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	plan := decode(t, r)
	if plan["canApply"] != false || plan["readOnly"] != true || plan["id"] == "" || plan["generation"] == nil {
		t.Fatal(plan)
	}
	bodies := []string{"", "{", "null", "[]", validProxy + ` {}`, strings.Replace(validProxy, `"nodeCount":2`, `"nodeCount":-1`, 1), strings.Replace(validProxy, `"nodeCount":2`, `"nodeCount":0`, 1), strings.Replace(validProxy, `"nodeCount":2`, `"nodeCount":4097`, 1), strings.Replace(validProxy, `"nodeCount":2`, `"nodeCount":1.5`, 1), strings.Replace(validProxy, `"mode":"split"`, `"mode":"bad"`, 1), strings.Replace(validProxy, `"nodeCount":2`, `"token":"secret","nodeCount":2`, 1)}
	for _, body := range bodies {
		r = request(t, ts, "POST", "/api/proxy/plan", body, nil)
		if r.StatusCode != 400 && r.StatusCode != 415 {
			t.Fatalf("body %q status %d", body, r.StatusCode)
		}
		if decode(t, r)["error"] == nil {
			t.Fatal("no envelope")
		}
	}
	r = request(t, ts, "POST", "/api/proxy/plan", `{"padding":"`+strings.Repeat("x", MaxBodyBytes)+`"}`, nil)
	if r.StatusCode != 413 {
		t.Fatal(r.Status)
	}
}

const validFRPC = `{"serverAddress":"relay.example.invalid","serverPort":7000,"tls":true,"transport":"tcp","proxies":[{"name":"test","type":"tcp","localAddress":"127.0.0.1","localPort":8080,"remotePort":18080}]}`

func TestFRPCValidation(t *testing.T) {
	_, ts := testServer(t, "")
	r := request(t, ts, "POST", "/api/frpc/plan", validFRPC, nil)
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	v := decode(t, r)
	if v["canApply"] != false || len(v["warnings"].([]any)) == 0 {
		t.Fatal(v)
	}
	for _, body := range []string{strings.Replace(validFRPC, `"serverPort":7000`, `"serverPort":0`, 1), strings.Replace(validFRPC, `relay.example.invalid`, `https://relay.example.invalid`, 1), strings.Replace(validFRPC, `"localPort":8080`, `"localPort":65536`, 1), strings.Replace(validFRPC, `"remotePort":18080`, `"token":"secret"`, 1), strings.Replace(validFRPC, `"type":"tcp"`, `"type":"http"`, 1), strings.Replace(validFRPC, `"transport":"tcp"`, `"transport":"bad"`, 1)} {
		r = request(t, ts, "POST", "/api/frpc/plan", body, nil)
		if r.StatusCode != 400 {
			t.Fatalf("%s: %s", body, r.Status)
		}
	}
}
func TestAuthentication(t *testing.T) {
	_, ts := testServer(t, "correct")
	r := request(t, ts, "GET", "/api/session", "", nil)
	v := decode(t, r)
	if v["authenticated"] != false || v["authRequired"] != true {
		t.Fatal(v)
	}
	for _, path := range []string{"/api/health", "/api/modules", "/api/events"} {
		r = request(t, ts, "GET", path, "", nil)
		if r.StatusCode != 401 {
			t.Fatal(r.Status)
		}
	}
	r = request(t, ts, "POST", "/api/session/login", `{"password":"wrong"}`, nil)
	if r.StatusCode != 401 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "POST", "/api/session/login", `{"password":"correct","unknown":true}`, nil)
	if r.StatusCode != 400 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "POST", "/api/session/login", `{"password":"correct"}`, nil)
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	cookies := r.Cookies()
	if len(cookies) != 1 || !cookies[0].HttpOnly || cookies[0].SameSite != http.SameSiteStrictMode || cookies[0].Path != "/" {
		t.Fatal(cookies)
	}
	r = request(t, ts, "GET", "/api/health", "", cookies[0])
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "POST", "/api/session/logout", "", cookies[0])
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "GET", "/api/health", "", cookies[0])
	if r.StatusCode != 401 {
		t.Fatal(r.Status)
	}
}
func TestOriginAndAttempts(t *testing.T) {
	_, ts := testServer(t, "correct")
	for _, origin := range []string{"https://evil.invalid", "null", ts.URL + "/path"} {
		req, _ := http.NewRequest("POST", ts.URL+"/api/session/login", strings.NewReader(`{"password":"correct"}`))
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Origin", origin)
		res, err := ts.Client().Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != 403 {
			t.Fatal(res.Status)
		}
	}
	req, _ := http.NewRequest("POST", ts.URL+"/api/session/login", strings.NewReader(`{"password":"correct"}`))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Origin", ts.URL)
	r, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	r.Body.Close()
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	for i := 0; i < 6; i++ {
		r = request(t, ts, "POST", "/api/session/login", `{"password":"wrong"}`, nil)
		want := 401
		if i == 5 {
			want = 429
		}
		if r.StatusCode != want {
			t.Fatalf("attempt %d: %s", i, r.Status)
		}
	}
}
func TestSSEDisconnect(t *testing.T) {
	srv, ts := testServer(t, "")
	req, _ := http.NewRequest("GET", ts.URL+"/api/events", nil)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	req = req.WithContext(ctx)
	res, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	if res.Header.Get("Content-Type") != "text/event-stream" {
		t.Fatal(res.Header)
	}
	reader := bufio.NewReader(res.Body)
	found := false
	for i := 0; i < 6; i++ {
		line, err := reader.ReadString('\n')
		if err != nil {
			t.Fatal(err)
		}
		if strings.HasPrefix(line, "data: ") {
			var v map[string]any
			if err := json.Unmarshal([]byte(strings.TrimPrefix(line, "data: ")), &v); err != nil {
				t.Fatal(err)
			}
			if v["system"] == nil || v["sampledAt"] == nil {
				t.Fatal(v)
			}
			found = true
			break
		}
	}
	if !found {
		t.Fatal("missing snapshot")
	}
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			t.Fatal(err)
		}
		if strings.HasPrefix(line, ": heartbeat") {
			break
		}
	}
	cancel()
	res.Body.Close()
	deadline := time.Now().Add(time.Second)
	for srv.sampler.SubscriberCount() != 0 && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if srv.sampler.SubscriberCount() != 0 {
		t.Fatal("subscriber leak")
	}
}
func TestMissingStaticKeepsAPI(t *testing.T) {
	observer := modules.NewSystem(true)
	sampler := core.NewSampler(observer.Observe, time.Hour)
	srv, err := New(Config{System: observer, Network: modules.Network{}, Sampler: sampler, WebDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	ts := httptest.NewServer(srv)
	defer ts.Close()
	r := request(t, ts, "GET", "/api/health", "", nil)
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "GET", "/", "", nil)
	b, _ := io.ReadAll(r.Body)
	if r.StatusCode != 503 || !strings.Contains(string(b), "web/dist") {
		t.Fatal(r.Status, string(b))
	}
}

func TestJSONContractEdges(t *testing.T) {
	_, ts := testServer(t, "")
	for _, body := range []string{
		`{"mode":"split","mode":"global","dnsStrategy":"split","ipv6Policy":"follow","failurePolicy":"direct","nodeCount":1}`,
		`{"mode":"split","Mode":"global","dnsStrategy":"split","ipv6Policy":"follow","failurePolicy":"direct","nodeCount":1}`,
		`{"mode":"split","dnsStrategy":"split","ipv6Policy":"follow","failurePolicy":"direct","nodeCount":null}`,
		`{"mode":"direct","dnsStrategy":"direct","ipv6Policy":"direct","failurePolicy":"direct"}`,
		`{"mode":"split","dnsStrategy":null,"ipv6Policy":"follow","failurePolicy":"direct","nodeCount":1}`,
	} {
		r := request(t, ts, "POST", "/api/proxy/plan", body, nil)
		if r.StatusCode != 400 {
			t.Fatal(r.Status, body)
		}
	}
	req, _ := http.NewRequest("POST", ts.URL+"/api/proxy/plan", strings.NewReader(validProxy))
	req.Header.Set("Content-Type", "text/plain")
	res, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 415 {
		t.Fatal(res.Status)
	}
	for _, body := range []string{
		strings.Replace(validFRPC, `"tls":true,`, ``, 1),
		strings.Replace(validFRPC, `"tls":true`, `"tls":null`, 1),
		strings.Replace(validFRPC, `"name":"test"`, `"name":"test","name":"duplicate"`, 1),
		strings.Replace(validFRPC, `"localPort":8080`, `"localPort":8080,"token":"private"`, 1),
	} {
		r := request(t, ts, "POST", "/api/frpc/plan", body, nil)
		if r.StatusCode != 400 {
			t.Fatal(r.Status, body)
		}
	}
}
func TestLogAPI(t *testing.T) {
	srv, ts := testServer(t, "")
	srv.logger.Info("Startup", "code", "server_started", "module", "core")
	r := request(t, ts, "GET", "/api/logs?limit=1", "", nil)
	if r.StatusCode != 200 {
		t.Fatal(r.Status)
	}
	v := decode(t, r)
	if v["capacity"] != float64(core.LogCapacity) || len(v["entries"].([]any)) != 1 {
		t.Fatal(v)
	}
	entry := v["entries"].([]any)[0].(map[string]any)
	if entry["code"] != "server_started" || entry["module"] != "core" || entry["time"] == nil || entry["sequence"] == nil || len(entry) != 6 {
		t.Fatal(entry)
	}
	for _, query := range []string{"limit=0", "limit=501", "limit=-1", "limit=x", "limit=1&limit=2", "unexpected=1", "limit=%zz"} {
		r = request(t, ts, "GET", "/api/logs?"+query, "", nil)
		if r.StatusCode != 400 {
			t.Fatal(query, r.Status)
		}
	}
	_, authed := testServer(t, "secret")
	r = request(t, authed, "GET", "/api/logs", "", nil)
	if r.StatusCode != 401 {
		t.Fatal(r.Status)
	}
}
func TestSessionExpiryAndSecureCookie(t *testing.T) {
	observer := modules.NewSystem(true)
	sampler := core.NewSampler(observer.Observe, time.Hour)
	srv, err := New(Config{System: observer, Sampler: sampler, Password: "secret"})
	if err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	now := time.Now()
	srv.auth.now = func() time.Time { return now }
	ts := httptest.NewTLSServer(srv)
	defer ts.Close()
	res := request(t, ts, "POST", "/api/session/login", `{"password":"secret"}`, nil)
	cookie := res.Cookies()[0]
	if !cookie.Secure {
		t.Fatal("TLS cookie not secure")
	}
	r := request(t, ts, "GET", "/api/session", "", cookie)
	if decode(t, r)["authenticated"] != true {
		t.Fatal("auth failed")
	}
	now = now.Add(sessionTTL + time.Second)
	r = request(t, ts, "GET", "/api/health", "", cookie)
	if r.StatusCode != 401 {
		t.Fatal(r.Status)
	}
}
func TestAuthMemoryBounds(t *testing.T) {
	a := newAuth("secret")
	now := time.Now()
	a.now = func() time.Time { return now }
	for i := 0; i < maxAttemptKeys+10; i++ {
		r := httptest.NewRequest("POST", "/", nil)
		r.RemoteAddr = fmt.Sprintf("192.0.2.%d:123", i)
		a.allowAttempt(r)
	}
	if len(a.failures) != maxAttemptKeys {
		t.Fatal("attempt keys unbounded", len(a.failures))
	}
	now = now.Add(attemptWindow + time.Second)
	r := httptest.NewRequest("POST", "/", nil)
	r.RemoteAddr = "198.51.100.1:123"
	if !a.allowAttempt(r) || len(a.failures) != 1 {
		t.Fatal("expired attempts retained")
	}
	for i := 0; i < maxSessions+10; i++ {
		if err := a.createSession(httptest.NewRecorder(), r); err != nil {
			t.Fatal(err)
		}
	}
	if len(a.sessions) != maxSessions {
		t.Fatal("sessions unbounded")
	}
	now = now.Add(sessionTTL + time.Second)
	if err := a.createSession(httptest.NewRecorder(), r); err != nil {
		t.Fatal(err)
	}
	if len(a.sessions) != 1 {
		t.Fatal("expired sessions retained")
	}
}
func TestFetchMetadataOrigin(t *testing.T) {
	_, ts := testServer(t, "")
	req, _ := http.NewRequest("POST", ts.URL+"/api/proxy/plan", strings.NewReader(validProxy))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Sec-Fetch-Site", "cross-site")
	res, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 403 {
		t.Fatal(res.Status)
	}
	req, _ = http.NewRequest("POST", ts.URL+"/api/proxy/plan", strings.NewReader(validProxy))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Origin", ts.URL)
	req.Header.Add("Origin", ts.URL)
	res, err = ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 403 {
		t.Fatal(res.Status)
	}
}
func TestSSELimitLogoutAndShutdown(t *testing.T) {
	srv, ts := testServer(t, "secret")
	res := request(t, ts, "POST", "/api/session/login", `{"password":"secret"}`, nil)
	cookie := res.Cookies()[0]
	var cancels []func()
	for i := 0; i < core.MaxSubscribers; i++ {
		_, cancel, err := srv.sampler.Subscribe()
		if err != nil {
			t.Fatal(err)
		}
		cancels = append(cancels, cancel)
	}
	r := request(t, ts, "GET", "/api/events", "", cookie)
	if r.StatusCode != 503 {
		t.Fatal(r.Status)
	}
	for _, cancel := range cancels {
		cancel()
	}
	r = request(t, ts, "GET", "/api/events", "", cookie)
	reader := bufio.NewReader(r.Body)
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			t.Fatal(err)
		}
		if strings.HasPrefix(line, "data:") {
			break
		}
	}
	logout := request(t, ts, "POST", "/api/session/logout", "", cookie)
	if logout.StatusCode != 200 {
		t.Fatal(logout.Status)
	}
	deadline := time.Now().Add(time.Second)
	for srv.sampler.SubscriberCount() != 0 && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if srv.sampler.SubscriberCount() != 0 {
		t.Fatal("logout did not invalidate stream")
	}
	r.Body.Close()
	// Closing the server terminates streams even before the HTTP listener stops.
	res = request(t, ts, "POST", "/api/session/login", `{"password":"secret"}`, nil)
	cookie = res.Cookies()[0]
	r = request(t, ts, "GET", "/api/events", "", cookie)
	srv.Close()
	_, err := io.ReadAll(r.Body)
	if err != nil {
		t.Fatal(err)
	}
	if srv.sampler.SubscriberCount() != 0 {
		t.Fatal("shutdown subscription leak")
	}
}
func TestStaticSPABoundary(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "index.html"), []byte("test app"), 0600); err != nil {
		t.Fatal(err)
	}
	observer := modules.NewSystem(true)
	sampler := core.NewSampler(observer.Observe, time.Hour)
	srv, err := New(Config{System: observer, Sampler: sampler, WebDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	ts := httptest.NewServer(srv)
	defer ts.Close()
	r := request(t, ts, "GET", "/network", "", nil)
	body, _ := io.ReadAll(r.Body)
	if r.StatusCode != 200 || string(body) != "test app" {
		t.Fatal(r.Status, string(body))
	}
	r = request(t, ts, "GET", "/missing.js", "", nil)
	if r.StatusCode != 404 {
		t.Fatal(r.Status)
	}
	r = request(t, ts, "GET", "/api/unknown", "", nil)
	if r.StatusCode != 404 || decode(t, r)["error"] == nil {
		t.Fatal(r.Status)
	}
}

package httpapi

import (
	"crypto/tls"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRemoteManagementUsesCallerOriginNotLANTuple(t *testing.T) {
	for _, scheme := range []string{"http", "https"} {
		request := httptest.NewRequest("POST", scheme+"://panel.example.net:7443/api/proxy/select", strings.NewReader(`{}`))
		request.Host = "panel.example.net:7443"
		request.Header.Set("Origin", scheme+"://panel.example.net:7443")
		if scheme == "https" {
			request.TLS = &tls.ConnectionState{}
		}
		if !sameOrigin(request) {
			t.Fatal("FRPC external host treated as foreign origin", scheme)
		}
		request.Header.Set("Origin", scheme+"://evil.example.net")
		if sameOrigin(request) {
			t.Fatal("remote CSRF origin accepted")
		}
	}
	request := httptest.NewRequest("POST", "http://panel.example.net/api/proxy/select", nil)
	request.Header.Set("Origin", "https://panel.example.net")
	request.Header.Set("X-Forwarded-Proto", "https")
	if sameOrigin(request) {
		t.Fatal("untrusted forwarded scheme granted origin authority")
	}
}

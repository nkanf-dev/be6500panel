package httpapi

import (
	"be6500panel/internal/capture"
	"context"
	"encoding/json"
	"errors"
	"net/http/httptest"
	"testing"
)

func TestCaptureDisableStorageFailureReturnsLatchedStatusAndRetryWarning(t *testing.T) {
	srv, _ := testServer(t, "")
	owner, err := capture.New(t.TempDir(), func(context.Context, []string) ([]byte, error) {
		t.Fatal("empty capture should not run kernel commands")
		return nil, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	owner.SetStorageAdmission(func(context.Context, string, int64, bool) (func(), error) {
		return nil, errors.New("private storage fixture failure")
	})
	srv.capture = owner
	response := httptest.NewRecorder()
	srv.ServeHTTP(response, httptest.NewRequest("DELETE", "/api/proxy/capture", nil))
	var payload struct {
		Error   apiError       `json:"error"`
		Capture capture.Status `json:"capture"`
	}
	if response.Code != 500 || json.Unmarshal(response.Body.Bytes(), &payload) != nil || payload.Error.Code != "capture_disable_not_persisted" || payload.Capture.Desired || payload.Capture.Error != "capture_disable_not_persisted" {
		t.Fatal(response.Code, response.Body.String())
	}
	if payload.Error.Message != "已停止自动恢复接管，但关闭状态尚未保存；请重试撤回。保存成功前不要重启面板或路由器。" {
		t.Fatal(payload.Error.Message)
	}
}

package httpapi

import (
	"be6500panel/internal/deviceannotations"
	"be6500panel/internal/devicetelemetry"
	"context"
	"encoding/json"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

type annotationActivitySource struct{ calls atomic.Int32 }

func (s *annotationActivitySource) Snapshot(context.Context) (devicetelemetry.Snapshot, error) {
	s.calls.Add(1)
	return devicetelemetry.Snapshot{SampledAt: time.Now().UTC(), Devices: []devicetelemetry.Observation{{ID: "02:00:00:00:00:7F", Name: "android-127", Interface: "fixture", Associated: true, Counters: []devicetelemetry.Counter{{Address: "192.0.2.128", RX: 100, TX: 200}}}}}, nil
}
func TestDeviceActivitySearchUsesPrivateAnnotationSnapshotOnlyForMatching(t *testing.T) {
	source := &annotationActivitySource{}
	collector, err := devicetelemetry.New(source)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	defer collector.Close()
	collector.Start(ctx)
	deadline := time.Now().Add(time.Second)
	for {
		snapshot, err := collector.Query(ctx, "24h", 288, 64, "")
		if err != nil {
			t.Fatal(err)
		}
		if snapshot.State == "ok" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("first source sample missing")
		}
		time.Sleep(time.Millisecond)
	}
	annotations, err := deviceannotations.New(deviceannotations.Options{DataDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	_, err = annotations.Save(ctx, deviceannotations.UpdateRequest{MAC: "02:00:00:00:00:7f", Label: "客厅电视", Note: "影视用途", Tags: []string{"media"}, ExpectedRevision: 0})
	if err != nil {
		t.Fatal(err)
	}
	for _, search := range []string{"客厅电视", "影视用途", "media", "android-127"} {
		request := httptest.NewRequest("GET", "/api/devices/activity?range=24h&limit=64&search="+url.QueryEscape(search), nil)
		response := httptest.NewRecorder()
		DeviceActivityWithAnnotations(response, request, collector, annotations)
		var history devicetelemetry.History
		if response.Code != 200 || json.Unmarshal(response.Body.Bytes(), &history) != nil || history.MatchedCount != 1 || len(history.Devices) != 1 || history.Devices[0].ID != "02:00:00:00:00:7F" {
			t.Fatal(response.Code, response.Body.String())
		}
		if strings.Contains(response.Body.String(), "客厅电视") || strings.Contains(response.Body.String(), "影视用途") {
			t.Fatal("matching private text leaked into telemetry")
		}
	}
	if source.calls.Load() != 1 {
		t.Fatal("browser search triggered source reads", source.calls.Load())
	}
}

package httpapi

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestPrivateStorageAdmissionProtectsExistingSubscription(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	dir := t.TempDir()
	file := filepath.Join(dir, "subscription.yaml")
	if err := os.WriteFile(file, []byte("old private data"), 0600); err != nil {
		t.Fatal(err)
	}
	denied := errors.New("insufficient shared storage")
	s.storageAdmission = func(ctx context.Context, path string, n int64, recovery bool) (func(), error) {
		if path != dir || n < int64(len("candidate")) || recovery {
			t.Fatal("invalid ordinary admission")
		}
		return nil, denied
	}
	if err := s.writePrivate(context.Background(), file, []byte("candidate"), false); !errors.Is(err, denied) {
		t.Fatal(err)
	}
	raw, _ := os.ReadFile(file)
	if string(raw) != "old private data" {
		t.Fatal("denial replaced accepted private data")
	}
	released := false
	s.storageAdmission = func(ctx context.Context, path string, n int64, recovery bool) (func(), error) {
		return func() { released = true }, nil
	}
	if err := s.writePrivate(context.Background(), file, []byte("candidate"), false); err != nil {
		t.Fatal(err)
	}
	if !released {
		t.Fatal("reservation not released")
	}
}

func TestPrivateOffSwitchUsesRecoveryAdmission(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	s.dataDir = t.TempDir()
	var recovery bool
	s.storageAdmission = func(ctx context.Context, path string, n int64, r bool) (func(), error) {
		recovery = r
		return func() {}, nil
	}
	if err := s.persistDesired("sing-box", false); err != nil {
		t.Fatal(err)
	}
	if !recovery {
		t.Fatal("safe off switch denied access to recovery reserve")
	}
}

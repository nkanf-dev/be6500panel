package main

import "testing"

func TestBindBoundary(t *testing.T) {
	for _, address := range []string{"127.0.0.1:8787", "127.0.0.2:8787", "[::1]:8787", "localhost:8787"} {
		if err := validateListen(address, ""); err != nil {
			t.Fatal(address, err)
		}
	}
	for _, address := range []string{":8787", "0.0.0.0:8787", "[::]:8787", "192.0.2.1:8787", "untrusted.invalid:8787"} {
		if err := validateListen(address, ""); err == nil {
			t.Fatal(address)
		}
		if err := validateListen(address, "secret"); err != nil {
			t.Fatal(address, err)
		}
	}
	if err := validateListen("bad", "secret"); err == nil {
		t.Fatal("invalid address accepted")
	}
}

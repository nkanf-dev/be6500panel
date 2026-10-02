package main

import (
	"context"
	"net"
	"testing"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

func TestFRPCReadinessDoesNotInventConnection(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := runtimeReadiness(func() *managedruntime.Manager { return nil })(ctx, managedruntime.FRPC); err != nil {
		t.Fatal(err)
	}
}
func TestListenerSmoke(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	conn, err := net.DialTimeout("tcp", listener.Addr().String(), time.Second)
	if err != nil {
		t.Fatal(err)
	}
	conn.Close()
}

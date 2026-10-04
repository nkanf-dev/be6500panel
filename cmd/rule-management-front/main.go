// rule-management-front serves current policy editing without taking core ownership.
package main

import (
	"context"
	"errors"
	"flag"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"be6500panel/internal/httpapi"
	"be6500panel/internal/storage"
)

func main() {
	if err := run(); err != nil {
		slog.Error("Rule management front stopped", "code", "front_failed", "error", err)
		os.Exit(1)
	}
}
func run() error {
	listen := flag.String("listen", "127.0.0.1:8788", "HTTP bind address; LAN exposure must be chosen by the trusted operator")
	webDir := flag.String("web-dir", "web/dist", "Current prebuilt browser assets")
	dataDir := flag.String("data-dir", "", "Same private settings directory as the original panel (required)")
	flag.Parse()
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	// This process measures ordinary write headroom only. It neither allocates
	// nor reclaims the original owner's emergency reserve or runtime lock.
	budget, err := storage.New(storage.Options{})
	if err != nil {
		return err
	}
	front, err := httpapi.NewRuleManagementFront(httpapi.RuleManagementFrontConfig{Context: ctx, DataDir: *dataDir, WebDir: *webDir, Logger: slog.Default(), StorageAdmission: budget.Admit})
	if err != nil {
		return err
	}
	defer front.Close()
	listener, err := net.Listen("tcp", *listen)
	if err != nil {
		return err
	}
	server := &http.Server{Handler: front, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 << 10, BaseContext: func(net.Listener) context.Context { return ctx }}
	stopped := make(chan error, 1)
	go func() { stopped <- server.Serve(listener) }()
	slog.Info("Rule management front listening; original panel remains core owner", "listen", *listen)
	select {
	case err := <-stopped:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	case <-ctx.Done():
		front.Close()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if err := server.Shutdown(shutdown); err != nil {
			_ = server.Close()
			return err
		}
		err := <-stopped
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	}
}

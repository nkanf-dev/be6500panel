package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strconv"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

// runtimeReadiness checks local listeners from an already validated native config.
// FRPC has no required local listener; its process state is distinct from tunnel connectivity.
func runtimeReadiness(manager func() *managedruntime.Manager) func(context.Context, string) error {
	return func(ctx context.Context, service string) error {
		if service != managedruntime.SingBox {
			return nil
		}
		raw, _, err := manager().Config(service)
		if err != nil {
			return err
		}
		var config struct {
			Inbounds []struct {
				Type    string `json:"type"`
				Listen  string `json:"listen"`
				Port    uint16 `json:"listen_port"`
				Network string `json:"network"`
			} `json:"inbounds"`
		}
		if json.Unmarshal(raw, &config) != nil {
			return errors.New("accepted listener configuration invalid")
		}
		listeners := []string{}
		for _, inbound := range config.Inbounds {
			if inbound.Type != "mixed" && inbound.Type != "tproxy" && inbound.Type != "direct" {
				continue
			}
			if inbound.Network == "udp" {
				continue
			}
			host := inbound.Listen
			if host == "" || host == "0.0.0.0" {
				host = "127.0.0.1"
			}
			if host == "::" {
				host = "::1"
			}
			if net.ParseIP(host) == nil || inbound.Port == 0 {
				return errors.New("native listener address invalid")
			}
			listeners = append(listeners, net.JoinHostPort(host, strconv.Itoa(int(inbound.Port))))
		}
		if len(listeners) == 0 {
			return errors.New("no managed proxy listener")
		}
		ticker := time.NewTicker(40 * time.Millisecond)
		defer ticker.Stop()
		dialer := net.Dialer{Timeout: 200 * time.Millisecond}
		for {
			ready := true
			for _, address := range listeners {
				conn, err := dialer.DialContext(ctx, "tcp", address)
				if err != nil {
					ready = false
					break
				}
				conn.Close()
			}
			if ready {
				return nil
			}
			select {
			case <-ctx.Done():
				return fmt.Errorf("listener readiness: %w", ctx.Err())
			case <-ticker.C:
			}
		}
	}
}

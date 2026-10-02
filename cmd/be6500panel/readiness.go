package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

const (
	readinessRetryInterval = 40 * time.Millisecond
	listenerProbeTimeout   = 200 * time.Millisecond
	// The managed core's DNS transport allows five seconds for a cold lookup.
	// A short listener-dial timeout would discard a working bootstrap response.
	dnsProbeTimeout = 6 * time.Second
)

type readinessTarget struct {
	network string
	address string
	domain  string // Empty for a non-DNS listener smoke check.
}

type nativeReadinessConfig struct {
	Inbounds []struct {
		Type    string           `json:"type"`
		Tag     string           `json:"tag"`
		Listen  string           `json:"listen"`
		Port    uint16           `json:"listen_port"`
		Network readinessStrings `json:"network"`
	} `json:"inbounds"`
	Route struct {
		Rules []struct {
			Action  string           `json:"action"`
			Inbound readinessStrings `json:"inbound"`
			Port    readinessPorts   `json:"port"`
		} `json:"rules"`
	} `json:"route"`
	DNS struct {
		Servers []struct {
			Tag    string `json:"tag"`
			Server string `json:"server"`
			TLS    struct {
				ServerName string `json:"server_name"`
			} `json:"tls"`
		} `json:"servers"`
		Rules []struct {
			Server string           `json:"server"`
			Domain readinessStrings `json:"domain"`
		} `json:"rules"`
	} `json:"dns"`
}

// Native sing-box listable fields accept either one value or an array.
type readinessStrings []string

func (s *readinessStrings) UnmarshalJSON(raw []byte) error {
	var one string
	if err := json.Unmarshal(raw, &one); err == nil {
		*s = []string{one}
		return nil
	}
	var many []string
	if err := json.Unmarshal(raw, &many); err != nil {
		return err
	}
	*s = many
	return nil
}

type readinessPorts []uint16

func (p *readinessPorts) UnmarshalJSON(raw []byte) error {
	var one uint16
	if err := json.Unmarshal(raw, &one); err == nil {
		*p = []uint16{one}
		return nil
	}
	var many []uint16
	if err := json.Unmarshal(raw, &many); err != nil {
		return err
	}
	*p = many
	return nil
}

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
		return checkNativeReadiness(ctx, raw)
	}
}

// Keep configuration selection separate from I/O so startup tests need no core,
// upstream DNS server, or transparent capture rules.
func nativeReadinessTargets(raw []byte) ([]readinessTarget, error) {
	var config nativeReadinessConfig
	if err := json.Unmarshal(raw, &config); err != nil {
		return nil, errors.New("accepted listener configuration invalid")
	}
	domain := config.readinessDomain()
	var targets []readinessTarget
	for _, inbound := range config.Inbounds {
		if inbound.Type != "mixed" && inbound.Type != "tproxy" && inbound.Type != "direct" {
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
			return nil, errors.New("native listener address invalid")
		}
		address := net.JoinHostPort(host, strconv.Itoa(int(inbound.Port)))
		udp, tcp := len(inbound.Network) == 0, len(inbound.Network) == 0
		for _, network := range inbound.Network {
			switch network {
			case "":
				udp, tcp = true, true
			case "udp":
				udp = true
			case "tcp":
				tcp = true
			default:
				return nil, errors.New("native listener network invalid")
			}
		}
		dns := false
		if inbound.Type == "direct" {
			for _, rule := range config.Route.Rules {
				if rule.Action != "hijack-dns" {
					continue
				}
				inboundMatches := len(rule.Inbound) == 0
				for _, tag := range rule.Inbound {
					inboundMatches = inboundMatches || tag == inbound.Tag
				}
				if !inboundMatches {
					continue
				}
				portMatches := len(rule.Port) == 0
				for _, port := range rule.Port {
					portMatches = portMatches || port == inbound.Port
				}
				dns = dns || portMatches
			}
			if !dns && (inbound.Tag == "dns-in" || inbound.Port == 53) {
				return nil, errors.New("native DNS listener missing hijack-dns route")
			}
		}
		if dns {
			if udp {
				targets = append(targets, readinessTarget{network: "udp", address: address, domain: domain})
			}
			if tcp {
				targets = append(targets, readinessTarget{network: "tcp", address: address, domain: domain})
			}
		} else if tcp {
			targets = append(targets, readinessTarget{network: "tcp", address: address})
		}
	}
	if len(targets) == 0 {
		return nil, errors.New("no managed proxy listener")
	}
	return targets, nil
}

func (c nativeReadinessConfig) readinessDomain() string {
	// Prefer a direct resolver's own hostname when explicitly bootstrapped.
	// This also works when the proxy node itself has only a literal IP.
	identity := ""
	for _, server := range c.DNS.Servers {
		if server.Tag != "dns-direct" {
			continue
		}
		identity = normalizedDNSReadinessDomain(server.TLS.ServerName)
		if identity == "" {
			identity = normalizedDNSReadinessDomain(server.Server)
		}
		if identity != "" {
			break
		}
	}
	first := ""
	for _, rule := range c.DNS.Rules {
		if rule.Server != "dns-direct" {
			continue
		}
		for _, domain := range rule.Domain {
			domain = normalizedDNSReadinessDomain(domain)
			if domain == "" {
				continue
			}
			if identity != "" && domain == identity {
				return domain
			}
			if first == "" {
				first = domain
			}
		}
	}
	if first != "" {
		return first
	}
	if identity != "" {
		return identity
	}
	// Compatibility fallback for explicit native configs without a bootstrap
	// rule. Managed configs explicitly route the resolver identity to dns-direct.
	return "dns.alidns.com"
}

func normalizedDNSReadinessDomain(domain string) string {
	domain = strings.ToLower(strings.TrimSuffix(domain, "."))
	if len(domain) == 0 || len(domain) > 253 || net.ParseIP(domain) != nil {
		return ""
	}
	for _, label := range strings.Split(domain, ".") {
		if len(label) == 0 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
			return ""
		}
		for _, b := range []byte(label) {
			if (b < 'a' || b > 'z') && (b < '0' || b > '9') && b != '-' {
				return ""
			}
		}
	}
	return domain
}

func checkNativeReadiness(ctx context.Context, raw []byte) error {
	targets, err := nativeReadinessTargets(raw)
	if err != nil {
		return err
	}
	ticker := time.NewTicker(readinessRetryInterval)
	defer ticker.Stop()
	dialer := net.Dialer{Timeout: listenerProbeTimeout}
	for {
		if err := ctx.Err(); err != nil {
			return fmt.Errorf("listener readiness: %w", err)
		}
		ready := true
		for _, target := range targets {
			if target.domain != "" {
				err = probeDNSReadiness(ctx, target.network, target.address, target.domain, dnsProbeTimeout)
			} else {
				var conn net.Conn
				conn, err = dialer.DialContext(ctx, target.network, target.address)
				if err == nil {
					conn.Close()
				}
			}
			if err != nil {
				ready = false
				break
			}
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

// Package proxy compiles private proxy data and builds network intent. It never
// downloads subscriptions, runs commands, or changes router state.
package proxy

import (
	"fmt"
)

const (
	CoreVersion          = "1.14.2"
	MaxSubscriptionBytes = 2 << 20
	MaxNodes             = 2048
	MaxRules             = 8192
)

type IPv6Mode string

const (
	IPv6Follow IPv6Mode = "follow"
	IPv6Direct IPv6Mode = "direct"
	IPv6Block  IPv6Mode = "block"
)

type FailurePolicy string

const (
	FailureDirect     FailurePolicy = "direct"
	FailureBlockProxy FailurePolicy = "block-proxy"
)

type CaptureScope string

const (
	CaptureScopeDevices CaptureScope = "devices"
	CaptureScopeGateway CaptureScope = "gateway"
)

type DatapathMode string

const (
	// DatapathTPROXY is retained only to withdraw old ownership journals.
	// CompileNative rejects this retired backend.
	DatapathTPROXY    DatapathMode = "tproxy"
	DatapathRoutedTUN DatapathMode = "routed-tun"
)

// RoutedTUNConfig is managed original-packet intent. CompileNative defaults a
// nil value to b6p-tun at 172.31.255.253/30 without changing caller input.
type RoutedTUNConfig struct {
	InterfaceName string `json:"interfaceName"`
	Address       string `json:"address"`
}

// Ports includes TProxy only for old ownership journals. CompileNative ignores
// it and validates the mixed and DNS listeners that routed TUN actually emits.
type Ports struct{ Mixed, TProxy, DNS uint16 }

// Node is private input. JSON and formatting deliberately do not expose node
// UUIDs or REALITY material. Use Public() for the authenticated UI.
type Node struct {
	ID               string `json:"id"`
	Name             string `json:"-"`
	Server           string `json:"-"`
	Port             uint16 `json:"-"`
	UUID             string `json:"-"`
	ServerName       string `json:"-"`
	RealityPublicKey string `json:"-"`
	RealityShortID   string `json:"-"`
	Fingerprint      string `json:"-"`
	Flow             string `json:"-"`
	UDP              bool   `json:"-"`
}

func (n Node) String() string   { return fmt.Sprintf("VLESS node %s (private)", n.ID) }
func (n Node) GoString() string { return n.String() }

type PublicNode struct {
	ID        string `json:"id"`
	Label     string `json:"label"`
	Server    string `json:"server"`
	Port      uint16 `json:"port"`
	Protocol  string `json:"protocol"`
	Transport string `json:"transport"`
	Reality   bool   `json:"reality"`
	Vision    bool   `json:"vision"`
	UTLS      bool   `json:"utls"`
	UDP       bool   `json:"udp"`
}

func (n Node) Public() PublicNode {
	return PublicNode{ID: n.ID, Label: n.Name, Server: n.Server, Port: n.Port, Protocol: "vless", Transport: "tcp", Reality: true, Vision: true, UTLS: true, UDP: n.UDP}
}

// Diagnostic contains safe fixed messages, never original YAML/credentials.
// Index is the zero-based node/rule position; -1 means subscription/config scope.
type Diagnostic struct {
	Scope   string `json:"scope"`
	Index   int    `json:"index"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

type RuleKind string

const (
	RuleDomain        RuleKind = "domain"
	RuleDomainSuffix  RuleKind = "domain-suffix"
	RuleDomainKeyword RuleKind = "domain-keyword"
	RuleIPCIDR        RuleKind = "ip-cidr"
	RuleMatch         RuleKind = "match"
	RuleSet           RuleKind = "rule-set"
)

type Target string

const (
	TargetDirect Target = "direct"
	TargetProxy  Target = "proxy"
	TargetBlock  Target = "block"
)

// Rule keeps subscription order. NoResolve is retained for diagnostics; native
// rules never perform eager resolution for a no-resolve CIDR rule.
type Rule struct {
	Kind      RuleKind `json:"kind"`
	Value     string   `json:"value,omitempty"`
	Target    Target   `json:"target"`
	NoResolve bool     `json:"noResolve,omitempty"`
	Index     int      `json:"index"`
}

type Subscription struct {
	Nodes       []Node       `json:"-"`
	Rules       []Rule       `json:"rules"`
	Diagnostics []Diagnostic `json:"diagnostics"`
	GroupCount  int          `json:"groupCount"`
	FakeIP      bool         `json:"fakeIP"`
}

func (s Subscription) PublicNodes() []PublicNode {
	out := make([]PublicNode, len(s.Nodes))
	for i, n := range s.Nodes {
		out[i] = n.Public()
	}
	return out
}
func (s Subscription) String() string {
	return fmt.Sprintf("private subscription: %d nodes, %d rules", len(s.Nodes), len(s.Rules))
}
func (s Subscription) GoString() string { return s.String() }

// DNSEndpoint is an IP-addressed authenticated TLS DNS server. A literal IP
// avoids recursion at bootstrap; ServerName is the certificate DNS identity.
type DNSEndpoint struct {
	Server     string
	Port       uint16
	ServerName string
}

// LocalDNSConfig preserves the original router dnsmasq authority. Server must
// be a literal loopback IP or an actual router address in ManagementIPs. Empty
// Server/Port defaults to 127.0.0.1:53. Domains and Hostnames extend the standard
// LAN suffixes and MiWiFi router aliases; unqualified lease names also stay local.
// The caller sources extra domains/aliases from the router, never subscription
// policy. dnsmasq must retain its original upstreams, not forward back to core.
// UDP transport retries truncated replies over TCP in sing-box 1.14.2.
type LocalDNSConfig struct {
	Server    string
	Port      uint16
	Domains   []string
	Hostnames []string
}

// RuleSetReference points to a locally staged, checksum-verified binary SRS.
// SourceURL is provenance only: CompileNative never downloads or embeds it.
// Kind is "domain" or "ip"; tags must be cn-domain/cn-ip/proxy-domain.
type RuleSetReference struct {
	Tag       string
	Kind      string
	Path      string
	SHA256    string
	SourceURL string
	MaxBytes  int64
}

// CompileInput is private. Bootstrap/management bypass always wins over Rules.
// Selected Node replaces subscription selector groups; this is not a group
// evaluator. Explicit overrides precede the ordered subscription Rules.
type CompileInput struct {
	Node                   Node               `json:"-"`
	Rules                  []Rule             `json:"-"`
	Overrides              []Rule             `json:"-"`
	RuleSets               []RuleSetReference `json:"-"`
	Endpoints              []string           `json:"-"`
	BootstrapDomains       []string           `json:"-"`
	ManagementIPs          []string           `json:"-"`
	Datapath               DatapathMode       `json:",omitempty"`
	RoutedTUN              *RoutedTUNConfig   `json:",omitempty"`
	IPv6                   IPv6Mode
	Failure                FailurePolicy
	Ports                  Ports
	ListenAddress          string
	MixedListenAddress     string
	TProxyListenAddress    string // Ignored; retained while internal callers migrate.
	DNSListenAddress       string
	DirectDNS              DNSEndpoint
	ProxyDNS               DNSEndpoint
	LocalDNS               *LocalDNSConfig `json:"-"`
	FakeIP                 bool
	AcceptUnsupportedRules bool
	Diagnostics            []Diagnostic
}

func (i CompileInput) String() string   { return "private native compiler input" }
func (i CompileInput) GoString() string { return i.String() }

type CompileOutput struct {
	Config           []byte        `json:"-"`
	SHA256           string        `json:"sha256"`
	CoreVersion      string        `json:"coreVersion"`
	Diagnostics      []Diagnostic  `json:"diagnostics"`
	EndpointHosts    []string      `json:"-"`
	RequiredFeatures []string      `json:"requiredFeatures"`
	IPv6             IPv6Mode      `json:"ipv6"`
	Failure          FailurePolicy `json:"failure"`
}

func (o CompileOutput) String() string {
	return fmt.Sprintf("private sing-box %s config sha256=%s", o.CoreVersion, o.SHA256)
}
func (o CompileOutput) GoString() string { return o.String() }

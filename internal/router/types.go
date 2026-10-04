// Package router provides bounded, read-only observations of an RN02 router.
// It never changes UCI configuration or returns wireless credentials.
package router

import "time"

// Snapshot is an observation. A failed module leaves its own zero/empty fields
// and adds an error; it does not discard successful modules.
type Snapshot struct {
	Platform  Platform      `json:"platform"`
	Devices   []Device      `json:"devices"`
	WiFi      []WiFi        `json:"wifi"`
	DNS       DNS           `json:"dns"`
	Firewall  Firewall      `json:"firewall"`
	Traffic   []Traffic     `json:"traffic"`
	Routes    []Route       `json:"routes"`
	SampledAt time.Time     `json:"sampledAt"`
	Errors    []ModuleError `json:"errors"`
}

type Platform struct {
	Model        string `json:"model"`
	Firmware     string `json:"firmware"`
	Kernel       string `json:"kernel"`
	Architecture string `json:"architecture"`
}

// CaptureObservation is a fresh LAN identity observation for capture decisions.
// Unlike Snapshot, it never uses cached data or reads firewall state.
type CaptureObservation struct {
	Devices     []Device `json:"devices"`
	LANPrefixes []string `json:"lanPrefixes"`
	// LANAddresses contains actual br-lan IPv4 and IPv6 addresses, not
	// arbitrary addresses inside LANPrefixes. It can validate DNS binds.
	LANAddresses  []string `json:"lanAddresses"`
	ManagementIPs []string `json:"managementIPs"`
	// InterfaceAddresses binds actual addresses to their observed interface.
	// It distinguishes the core's own TUN address from a foreign collision.
	InterfaceAddresses []CaptureInterfaceAddress `json:"interfaceAddresses,omitempty"`
}

type CaptureInterfaceAddress struct {
	Interface string `json:"interface"`
	Address   string `json:"address"`
}

type Device struct {
	IP        string `json:"ip"`
	MAC       string `json:"mac"`
	Hostname  string `json:"hostname"`
	Eligible  bool   `json:"eligible"`
	Interface string `json:"interface,omitempty"`
	// Lease distinguishes current DHCP leases, including infinite leases,
	// from ARP-only observations. It is internal identity provenance.
	Lease bool `json:"-"`
	// ExpiresAt is nil for an infinite lease or an ARP-only observation.
	ExpiresAt *time.Time `json:"expiresAt"`
	// Online means a complete entry is present in /proc/net/arp. It is not a
	// reachability probe; DHCP lease validity alone does not imply online.
	Online bool `json:"online"`
}

type WiFi struct {
	Name string `json:"name"`
	SSID string `json:"ssid"`
	Band string `json:"band"`
	// Channel is 0 for auto/unknown. Fields describe configured safe UCI
	// values, not an inferred runtime channel for auto selection.
	Channel    int    `json:"channel"`
	Bandwidth  string `json:"bandwidth"`
	Disabled   bool   `json:"disabled"`
	Encryption string `json:"encryption"`
}

type DNS struct {
	Resolvers  []string `json:"resolvers"`
	LeaseCount int      `json:"leaseCount"`
}

type Firewall struct {
	IPv4 FirewallFamily `json:"ipv4"`
	IPv6 FirewallFamily `json:"ipv6"`
}

// FirewallFamily counts rules across tables. Policies come from filter.
// An empty policy means the source was unavailable, not ACCEPT.
type FirewallFamily struct {
	Input   string `json:"input"`
	Forward string `json:"forward"`
	Output  string `json:"output"`
	Rules   int    `json:"rules"`
}

type Traffic struct {
	Interface        string  `json:"interface"`
	RXBytes          uint64  `json:"rxBytes"`
	TXBytes          uint64  `json:"txBytes"`
	RXBytesPerSecond float64 `json:"rxBytesPerSecond"`
	TXBytesPerSecond float64 `json:"txBytesPerSecond"`
}

type Route struct {
	Family      string `json:"family"`
	Destination string `json:"destination"`
	Gateway     string `json:"gateway"`
	Interface   string `json:"interface"`
	Metric      uint64 `json:"metric"`
}

// ModuleError contains a stable code and a safe message, never raw command,
// parser, UCI, or operating system error text. Modules may succeed partially.
type ModuleError struct {
	Module  string `json:"module"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

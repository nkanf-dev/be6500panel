# Routed-TUN backend API contract

- **DatapathModes**: ['tproxy(default/empty)', 'routed-tun(explicit)']
- **ReservedPorts**: Ports.tproxy remains required7893 as compatibility field with routed-TUN, not an actual listening port; mixed/DNS deriveaccepted values
- **implicitSelection**: Missingdatapath preservesactualsupportedacceptedbackend; unsafeextraTUNfieldsfailwithoutnormalization
- **TUNConfig**: interfaceName safeownedb6p-;RFC1918firstusableIPv4/30 withnext;noauto routes;IPv6directinitialonly
- **activation**: Configure backend doesnotactivatecapture; capture desired must be explicit and guardian/readinessqualified; GETnevermutates
- **limitations**: IPv6/QUIC/VPN/fullLAN/completecrashavailability not inferred from diagnosticTCP/UDP; productionguardianpending

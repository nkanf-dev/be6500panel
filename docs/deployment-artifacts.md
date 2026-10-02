# Deployment artifacts

The panel is published as a signed/checksummed GitHub release artifact generated from public source. Runtime sing-box and FRPC binaries are separate artifacts with explicit upstream notices; credentials are never included. Initial ARM packages and compiled core are installed privately by SSH.

The launcher keeps the last accepted panel archive in `/data`, reconstructs it under `/tmp` and starts LAN-bound authenticated control. Large core artifacts are downloaded to `/tmp` from fixed checksum-pinned URLs. Small native settings, operation journal and desired-runtime metadata persist. Missing artifacts produce rebuilding state, not a fabricated running state.

For sing-box, `GPL-3.0-or-later` notices and fixed source/build recipe accompany the minimal binary. FRPC is upstream Apache-2.0. CN rule data is fetched from official source, not bundled in the MIT panel repo: rule-generation programs are GPL-3.0-or-later; domain data has upstream MIT attribution; GeoLite2-derived IP data has MaxMind's data/EULA requirements. Follow update and removal requirements instead of treating a pinned geolocation asset as forever-valid.

## RN02 bootstrap TLS adapter

The observed network can reach official GitHub API artifact URLs but transfers are slow. The deployment flag `--artifact-transport curl` uses the stock router's certificate-verified curl TLS stack, streams artifact bytes and still enforces the runtime SHA256/size checks. Binary API requests send `Accept: application/octet-stream`. The adapter has a six-minute download budget; UI acquisition uses a seven-minute request budget. Native HTTP remains the default for normal hosts. No TLS verification is disabled.

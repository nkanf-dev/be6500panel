# Bounded HTTPS Artifact Source Implementation Plan

> **For agentic workers:** Root-only transport/runtime integration. Native dependency checks, host and ARM builds run serially in sharedtree; no router operation.

**Goal:** Supply verified artifact Stage with actual certificate-verified HTTPS bytes under one absolute deadline and finite redirect/size limits.

**Architecture:** Rustls supplies TLS; mature ring cryptography is used rather than an incomplete alpha pure-Rust provider. This is a securitydependency, not applicationC implementation; no custom crypto or disabled certificatevalidation. Existing strict endpointDNS supplies hostname addresses from a fixed literal localDNS adapter, not getaddrinfo/backgroundthread. Nonblocking socket IO honors cancel/absolute deadline using boundedpoll. httparse validates bounded HTTPresponse framing. One responsebody reader streams into existingStage; no fullbody/encodedtemporary duplicate.

**Tech Stack:** Rust1.93/std/libc, rustls0.23 withring, webpki-roots, httparse and boundedHTTPURI parser; existing endpointDNS and Stage. No Tokio/webframework/systemresolver.

- [x] Pin native cratefeatures (no aws-lc/defaultlogging/Czlib); check host/ARM ABI crossbuild with existingclang+llvmtools. Ring'snecessary vettedcryptoC/assembly isnotnewprojectC; if crosscompilerblocks reporttruthfully, do not substituteexperimentalprovider.
- [x] Private typed SourcePolicy: HTTPSonlyproduction; testnumericloopbackHTTP explicit; literalbootstrap, boundedheaders16KiB/64,URLs4096,redirects10,body16MiB,absolute90sdeadline. URIcredentials/fragments/control/mappedzone/badscheme reject; eachredirect revalidated, noHTTPdowngrade orfile. No logs of URL/config.
- [x] OSrandomDNSnonce, A/AAAA strict terminalanswer validation; finiteaddressdial budget, certificateSNI/hostname/chain validation and TLS1.2/1.3. Fixedpollslice50ms/cancelbeforeafterIO. No dynamicpublicDNS or implicitproxyenvironment.
- [x] ResponseHTTP200only; strictuniquecontentlength/transferencoding, identityContentEncoding, finitechunkframing/trailers and headcap. Unexpectedcompressedtransportrefused; sha hashes exactbodybeforeartifactgzip. Connectionclose/no reuse; allreadsabsolute deadline; limit exact+onebyteprobe.
- [x] SyntheticHTTP/TLS localfixtures with checkedtestCA: validcert/wronghost/untrusted/expiry, redirects/downgrade/credentials, chunks/truncated/ambiguousheader/oversize/deadline/cancel, malformedDNS refused. FixturesincludeactualchecksumStage. TLSauthority fixturesnotpublicprodCAoverrides.
- [x] Nativefulltest/fmt/clippy/diff+ARMserial. Keep HTTPAcquire/defaultmainstartupunavailable until sourceandmanagerqualifiedtogether; no productionmigration/performanceclaim.

## Result

365 full serialized host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 crossbuild passed. Native TLS certificate validation retained; fixed post-handshake transport classification regression passes. Native HTTPS source is qualified in isolated fixtures; HTTPAcquire and defaultmainstartup/productionhandover remain unbound. No live router/core/capture operation or targetRSSclaim.

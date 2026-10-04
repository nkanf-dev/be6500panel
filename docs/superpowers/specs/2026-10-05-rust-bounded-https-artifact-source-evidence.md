# Bounded certificate-verified artifact source

## Transport and trust

SourcePolicy native enables HTTPS only, packaged Mozilla roots, TLS1.2/1.3 and hostname/chain verification. Mature rustls/ring supplies crypto; no project-owned C application, customcrypto, alphaTLSprovider, certificatebypass, proxyenvironment, DNSworker or backgrounddownload. PlainHTTP is only an explicitnumericloopback fixture mode; HTTPSredirectnever downgrades. TestCA injection remains private cfgtest and testprivatekeys are synthetic, never productioncredentials.

Hostname lookup uses accepted literalbootstrap with existingstrictA/AAAA answer/CNAME/nonce validation. Literals skipDNS, nonblockingliteralconnect reuses existingfiniteTCPdial. One socket and one absolute deadline for DNS/connect/TLShandshake/request/headers/body/redirects;cancelchecks everyblockedIOslice50ms. Fetchlimit90s/redirects10; TLSsendbuffers16KiB/maxfragment8KiB; cumulativeTLSencryptedhandshake256KiB and upstreamRustls messagebound65KiB. No idle sessions/resumption or connectionpool.

TLStransportEOF cannot spin. Actualread/write nonblock andpoll, compressedsourcebody streamsinto existingStage8KiB without encodedtempfile/fullbody allocation. SourceStage verifiesencodedSHA plusallgzipmembers/extractedhash; preliminaryartifactmetadata validation happensbeforeDNS/socket. Errorclasses private/fixed; transporterror classes are captured belowHTTPframing and surviveStagewrapper without rawOS/URL diagnostics. A regression uses realauthenticatedhandshake followed by an invalidTLSrecord in head/body phase; it first exposedHttpmisclassification, then passes asTls. Deadline/cancel remainpriority.

## HTTP framing and compatibility

ClosedURLs4096bytes withcredentials/fragments/controls/mappedzones/badports refused. EveryLocation relative/query/network/absolute is normalized/revalidated. Header16KiB/64 strictCRLF, uniqueContentLength/TransferEncoding, CL+TE refused; onlyidentityContentEncoding, no transparentHTTPdecompression. Body16MiB exact+probe, chunkline1024/chunks65536/trailers16KiB64, strictchunkextensions/trailerfields. Prefetchedbodybytes surviveheadparse. KnownCL/chunkcomplete returnsEOF withoutanotherreaderwait ifnoheldextra; prefetchedextra isFramingerror. Socketisneverreused; closeframingstillrequiresactualEOF.

## Qualification

Focused14framing+10actualsource tests pass. They cover certifiedlocalTLS hostname-boundrequest/SNI, wronghostnamehandshake/untrusted/expiry rejection, syntheticA/AAAA bootstrap, explicitHTTP/chunkedredirect, finite redirectloop, malformed/truncated/encodedbody/digestrefusal,peerEOF and absoluteheader/cancel interruption. A badsynthetickeygeneration fixture initiallyfailedbeforeTLS; regeneratedexplicitprime256v1 PKCS8. NativeTLSverification wasnotweakened.

MatureTLSdependencyhost/ARMcrossbuild passed beforeactualsourcegate. AppleClang needsfreestandingbuiltinheaders (undefheader-only__musl__selection macro), explicitARMv7-A VFPv3-D16 hardfloat. RustmuslABI/staticlink unchanged. Rootserializedhostfull andlatestARMsourceevidence are saved externally as rust-bounded-https-artifact-source-*; until completed/readback those finalresults remainpending. CI explicitlyprovidesARM C compiler/ar fornecessaryvettedcrypto dependency, not projectCcode. No live device/core/capture action, publicdownloadinterface/runtimeHTTPAcquire/productionstartupbinding or targetRSS/latencygain.

# Authenticated Capture HTTP Implementation Plan

**Goal:** Bind fixed capture GET/POST/DELETE to same manager/sharedcapturehooks without currentGET repair or oldjournal activation.

**Architecture:** RuntimeHttp accepts only typed CaptureHandle from actualinprocess hooks; no callbacks/argv/PID/root inHTTPbody. GET/HEAD pureprojection+finitecurrentreadonlyproof; POSTreadyownercontext selects fresh gateway orstableMAC scope thenpersistsintent/apply/preflight/native+installedreadback; DELETEofflatch/persistence thenbest-effortcleanup independentofcore. Defaultmainhasnocaptureattachment/unavailable503. NativeLAN/gateway derivescurrentprefixes internally, never browserranges. Sharedhookcontextmutation cannot start/restartcore. Observationfailureunknown andcleanupfailure retained.

- [x] Add boundedDELETE method parser acceptedONLY /api/proxy/capture; zero/nonzeroContentLength rules/bodymedia/sameorigin/auth required. RejectDELETEsession/static/otherAPI405withoutsideeffects, responseHEADzero body. ExistingGET/POSTcontracts unchanged.
- [x] Typedrequest captureobjectfields scope/devices/clientIPv4/clientIPv6/ipv6 strictunique/maponly,64devices/64KiBbody. OnlydirectIPv6/routedTUN. Gateway rejectsclient/device/ranges; snapshotLANdeclarescurrentprefix andfallbackliteralIP requirescurrentuniqueeligibleMAC, never persistedIP authority.
- [x] SharedCaptureHandle selection/disable/readstatus methods; CaptureRuntime preparation freshbuilder validatedbeforedesiredsave/oldcleanup. Selectionfailure/durability keeps truthfulintent/journal; disablinglatchesoff firstevenpersistence/executorfailure andretainsretryauthority. Applybudget honorsoutercaptureoperationdeadline (<=30s), failedapplycleanupindependent30s.
- [x] CaptureHTTPwire currentactive/state/desired/selectedclients/installedscope/commandcount/cleanup/error truthful, maxresponsebounded. No fabricatedtraffic/hits/internethealth. GET queries<=3s; HEADdoesnotperformprobe ormutation. POST/DELETE response<=65s allowownedcleanup.
- [x] ActualfakeHTTP+core/command tests auth/origin/DTO/noowner/nonready/GET/HEADzero mutation; gatewayfreshprefix/devices currentMAC; DELETEstopintent evenfailure withcorePID unchanged; queryfailure/cleanupfailure not falseinactive; sharedManageroneowner. Nativeproof separatelyqualified. Fullserialhost/static+ARM; no devicesoperation/productionbindingparityclaim.

## Result

421 full serialized host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 build passed. Actual retained native proof is repeated immediately before old-resource withdrawal and new Apply; both ownership-loss windows have regression coverage. GET/HEAD and DELETEofflatch semantics qualified in synthetic tests only. No device/network actions, defaultmainbinding or targetgain claim.

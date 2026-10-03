# Device workspace integration

## HTTP and persistent storage

Create one store at panel startup. Use the panel shared storage admission owner.

```go
store, err := deviceannotations.New(deviceannotations.Options{
    DataDir: *dataDir,
    StorageAdmission: flashBudget.Admit,
})
annotations := httpapi.DeviceAnnotationsHandler(store)
```

Register `httpapi.DeviceAnnotationsPath` (`/api/devices/annotations`) for GET and POST only. Dispatch the handler **after** the existing session authentication and same-origin checks. The helper is not a standalone public endpoint. Add root route tests for unauthorized reads/writes, rejected cross-origin POST, and accepted authenticated GET/POST.

GET and successful POST return `{revision, devices}`. `devices` is keyed by canonical uppercase six-byte MAC. Each entry is `{label, note, tags}`. POST requires `{mac, label, note, tags, expectedRevision}`. A conflicting revision returns HTTP 409 `revision_conflict`. All empty fields delete the annotation. An empty label alone restores the system-name/MAC title while retaining notes/tags.

The file is `<DataDir>/device-names.json`, normally `/data/be6500panel/device-names.json`. The directory is 0700 and the file is 0600. Writes use a same-directory temporary, full candidate storage admission, sync, and atomic rename. Rename is the commit point. Cancellation or I/O failure before rename preserves the accepted file. Read/write bounds are 256 MACs, 80 label characters, 1000 note characters, and 8 tags of 32 characters. Storage admission is a normal write, including deletion, and keeps the recovery reserve intact.

Notes are plaintext content entered by the LAN administrator. The notes flow does not scan for passwords or other content. Notes are not part of the configuration backup/export scope. A future explicit annotation export scope can include them deliberately.

## Authenticated console

Mount `DeviceLabelsProvider` from `web/src/modules/devices` once under the authenticated console. Do not create one provider for every chart. Unmount it on logout. The provider also clears names and cancels requests on `be6500panel:unauthorized` and `be6500panel:logout`. It keeps data only in session memory and never writes localStorage/sessionStorage.

Use `useDeviceLabels().displayName(mac, systemHostname)` in all device titles, routing/capture selectors, and charts. Display order is custom label, system hostname, canonical MAC. `DeviceLabel` offers the same lookup as a small component. Saves replace the shared snapshot and dispatch `be6500panel:device-labels-changed` for non-context consumers. Saving is explicit; search, selection, comparison, and page navigation do not write annotations or network settings.

## Workspace wiring

Mount `DeviceWorkspace` in the device page in place of the old narrow observation table. Pass:

- `snapshot`, `snapshotError`: the existing router observation.
- `activity`, `activityError`: `DeviceActivityHistory` from the trafficd collector.
- `proxy`, `proxyError`: the existing passive `ProxyMetrics` source. No active probe is needed.
- `range`, `onRangeChange`: `30m`, `24h`, or `7d`.
- `loading`, `onRefresh`: refresh the observations, not a configuration action.
- `selectedMAC`, `onSelectDevice`: optional dashboard drilldown selection.
- `onSelectedDevicesChange`: stable callback with zero to eight exact MACs currently needed for detail or comparison.
- `onConfigure("dhcp" | "firewall", mac)`: navigate to existing static-address or port-mapping configuration. The shortcut does not write or apply a draft.
- `renderActions(device)`: optional root-owned confirmed network actions, supported for the selected MAC. Do not insert generic arbitrary shell/service actions.

The base activity query returns at most 64 collector devices. Router DHCP/ARP observations still provide the remaining MAC list. For selected devices omitted by this cap, load exact MAC queries using `/api/devices/activity?range=...&search=<canonicalMAC>&limit=1&maxPoints=168`. Bound concurrent detail requests to the callback's at-most-eight MACs. Merge results with `mergeDeviceHistories(base, details, range)` before passing them to the workspace. Abort detail requests on selection/range/session change. The helper excludes other ranges and does not replace a newer per-device observation with an older one.

The workspace keeps a selected MAC visible through list search/filter/page changes. Inventory pages contain at most 25 rows. Comparison uses two to eight selected MACs with identical chart units and range. A range change shows a waiting state until that range is returned; old-range counters are not relabeled.

## Observation semantics

- Only valid MAC identities become editable rows. IP-only rows are not treated as permanent device identity.
- Multiple leases/addresses for one MAC are merged. Addresses remain visible as inventory evidence.
- Core connections correlate only by fresh current source-IP ownership that maps to one MAC. Each address keeps its own observation time. Trafficd address freshness uses the device row `lastSeen`, not the global history sample time. Old addresses do not become fresh when another device is sampled.
- Core source age is visible. Missing, expired, conflicting, or failed observations produce no attributed connection count. Global controller totals are not device request counts.
- A device history gap/reset is a null chart value, not a measured zero. Range totals are displayed only when effective coverage is positive.
- Wireless signal/noise, protocol, negotiated speed, vendor, MLO, and online time appear only when exported by the actual source. Absent radio measurements have concrete absent labels; no fabricated signal value is shown.
- Default kernel routes are a separate route reference, not proof of a selected device's proxy routing. Connection outbound comes from actual core rows.

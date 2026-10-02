# Control connectivity safety implementation plan

Goal: Keep automatic rollback supervised after recovery failures and preserve
management reachability and factory firewall behavior in native transactions.
Architecture: Retain the manager worker across failures, with timed backoff and
wake signals. Keep native firewall fields editable, but classify actual input
and interface connectivity risks and preserve existing factory includes.
Tech stack: Go standard library; synthetic root files and injected hooks only.

## Batch 1: Deadline recovery supervision

- [x] Add a fixture regression: first timeout rollback fails, manual rollback
  succeeds, and a second risky unconfirmed commit rolls back automatically.
- [x] Add recovery retry and non-spinning timing coverage.
- [x] Implement bounded automatic retry delay without ending the worker.
- [x] Document failure visibility, retry timing, and manual recovery behavior.
- [x] Run focused tests, race tests and vet; commit this coherent batch.

## Batch 2: Firewall connectivity and factory preservation

- [x] Test management input deny rules with both source and destination filters.
- [x] Test zone network/device membership and input-policy risk classification.
- [x] Preserve editable forwarding and disabled-rule fields when not risky.
- [x] Test preservation, reordering, deletion, mutation and duplicate includes.
- [x] Compare existing and candidate include multisets in both directions.
- [x] Document rules and include ownership; run test, race and vet, then commit.

No commands in this plan connect to a router or use captured live documents.

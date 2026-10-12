#!/bin/sh
# Disposable local synthetic demo; no employer or provider actions.
set -eu
binary="${JOB_HARNESS_BINARY:-job-harness}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
"$binary" --root "$work/source" init
"$binary" --root "$work/source" call bootstrap <<'JSON'
{"operation":"reconcile","capabilities":[]}
JSON
"$binary" --root "$work/source" call query_batch <<'JSON'
{"steps":[{"id":"answers","tool":"read_record","arguments":{"path":"applications/approved-answers.md"}},{"id":"decisions","tool":"read_record","arguments":{"path":"context/profile/application-decisions.md"}}]}
JSON
"$binary" --root "$work/source" call execute_batch <<'JSON'
{"steps":[{"id":"run","tool":"start_run","arguments":{"operation":"reconcile","scope":{"dry_run":true},"user_request":"Demonstrate local synthetic coordination","idempotency_key":"demo:start"}},{"id":"finish","tool":"finish_run","arguments":{"run_id":{"$ref":"run.data.run_id"},"status":"completed","summary":"Synthetic coordination demonstrated; no external actions","evidence":[],"idempotency_key":"demo:finish"}}]}
JSON
"$binary" --root "$work/source" workspace index
"$binary" --root "$work/source" workspace export --output "$work/demo.jhb"
"$binary" --root "$work/restored" workspace import --input "$work/demo.jhb"

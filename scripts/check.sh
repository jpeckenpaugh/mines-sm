#!/usr/bin/env bash
#
# Run quality gates and print a readable summary.
#
# `cargo clippy` is intentionally silent when there are no findings, so this
# wrapper always reports counts and a final verdict.
#
# Usage:
#   ./scripts/check.sh              # fmt + clippy + tests (debug, fast)
#   ./scripts/check.sh --release    # same, compiled in release
#   ./scripts/check.sh --exhaustive # release + #[ignore]d exhaustive suites
#
# The exhaustive suites enumerate billions of boards and are therefore NEVER run
# unless you ask for them explicitly with --exhaustive. Normal coverage comes
# from deterministic sampling.
#
set -uo pipefail
cd "$(dirname "$0")/.."

release=0
exhaustive=0
case "${1:-}" in
    "") ;;
    --release) release=1 ;;
    --exhaustive) release=1; exhaustive=1 ;;
    *)
        echo "usage: $0 [--release|--exhaustive]" >&2
        exit 2
        ;;
esac

test_label="cargo test"
if [ "$release" -eq 1 ]; then
    test_label="cargo test --release"
fi

fail=0

echo "==> cargo fmt --all -- --check"
if cargo fmt --all -- --check; then
    echo "    fmt: ok"
else
    echo "    fmt: FAILED"
    fail=1
fi

echo
echo "==> cargo clippy --all-targets"
clippy_log=$(cargo clippy --all-targets 2>&1)
errors=$(grep -c '^error' <<<"$clippy_log")
warnings=$(grep -c '^warning' <<<"$clippy_log")
if [ "$errors" -gt 0 ] || [ "$warnings" -gt 0 ]; then
    grep -E '^error|^warning' <<<"$clippy_log" | head -n 50
    fail=1
fi
echo "    clippy: ${errors} errors, ${warnings} warnings"

echo
echo "==> $test_label"
if [ "$release" -eq 1 ]; then
    test_log=$(cargo test --release 2>&1)
else
    test_log=$(cargo test 2>&1)
fi
grep -E 'Running|Doc-tests|test result:' <<<"$test_log"
passed=$(grep -oE '[0-9]+ passed' <<<"$test_log" | awk '{s+=$1} END{print s+0}')
failed=$(grep -oE '[0-9]+ failed' <<<"$test_log" | awk '{s+=$1} END{print s+0}')
if [ "$failed" -gt 0 ]; then
    fail=1
fi
echo "    tests: ${passed} passed, ${failed} failed"

if [ "$exhaustive" -eq 1 ]; then
    echo
    echo "==> cargo test --release -- --ignored (exhaustive suites; slow)"
    ignored_log=$(cargo test --release -- --ignored 2>&1)
    grep -E 'Running|test result:' <<<"$ignored_log"
    if grep -q 'FAILED' <<<"$ignored_log"; then
        fail=1
    fi
fi

echo
if [ "$fail" -eq 0 ]; then
    if [ "$exhaustive" -eq 1 ]; then
        echo "ALL CHECKS PASSED  (clippy clean, ${passed} tests passed, exhaustive suites run)"
    else
        echo "ALL CHECKS PASSED  (clippy clean, ${passed} tests passed)"
    fi
else
    echo "CHECKS FAILED"
fi
exit "$fail"

#!/usr/bin/env bash
# RCCE2 test runner for macOS / Linux. Mirrors test.bat.
# Compiles every test source in src/Tests with `blitzcc -t`. Any failure marks
# the run as failed but the loop continues so you see every broken test in one
# pass.
set -uo pipefail

ROOTDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BLITZPATH="${ROOTDIR}/compiler/BlitzForge"

if [[ -d "${ROOTDIR}/src/Tests" ]]; then
  TESTDIR="${ROOTDIR}/src/Tests"
elif [[ -d "${ROOTDIR}/src/tests" ]]; then
  TESTDIR="${ROOTDIR}/src/tests"
else
  echo "Test directory not found. Expected src/Tests or src/tests." >&2
  exit 1
fi

BLITZCC_UNIX="${BLITZPATH}/bin/blitzcc"
BLITZCC_WIN="${BLITZPATH}/bin/blitzcc.exe"
if [[ -x "${BLITZCC_UNIX}" ]]; then
  BLITZCC="${BLITZCC_UNIX}"
elif [[ -f "${BLITZCC_WIN}" ]]; then
  BLITZCC="${BLITZCC_WIN}"
else
  echo "${BLITZPATH}/bin/blitzcc not found!" >&2
  echo "Compile source with ./compile.sh -b, or download binaries from" >&2
  echo "  https://github.com/RydeTec/blitz-forge/releases" >&2
  exit 1
fi

cd "${TESTDIR}"

FAILED=0
while IFS= read -r -d '' f; do
  if ! "${BLITZCC}" -t -w "${ROOTDIR}/src" "${f}"; then
    echo "\"${f}\" failed at least one test"
    FAILED=1
  fi
done < <(find "${TESTDIR}" -type f -name '*.bb' -print0)

cd "${ROOTDIR}"

if [[ "${FAILED}" -eq 1 ]]; then
  echo "Tests failed"
  exit 1
fi

echo "Tests passed"

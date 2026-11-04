# Htrace 3.0.0 historical validation — 2026-11-04

Local working state, with no commit or publication. Rust stable 1.99.0,
Linux x86_64. Cargo offline with pinned versions; the only dependency removed
by this implementation was singletonThread 2.1.0. The maintainer's pre-existing
local changes were preserved.

This report records checks from that dated working state. It does not assert
that the commands were rerun for every later documentation or API revision.

## Tests

| Check | Result |
| --- | --- |
| `cargo test --offline --locked` | PASS: 24 unit tests + 5 integration tests + 3 doctests = 32 |
| `cargo test --offline --locked --all-features` | PASS: 25 unit tests + 9 integration tests + 3 doctests = 37 |
| Effective feature matrix | PASS: 12 configurations |
| Stripped release binary, 5 ERROR traces | PASS: 5 expected / 5 received, no timeout |
| Configured nightly Rustfmt, modified files only | PASS |
| `git diff --check` | PASS |
| `cargo clippy --offline --locked --all-targets --all-features` | PASS, 49 library warnings |

The matrix covers all eight subsets of default_module/log_consumer/
tracing_consumer with no implicit defaults, then hconfig with all four
log/tracing combinations. hconfig implies default_module. The default and
all-features commands were also run explicitly. A final behavior-preserving
change to the Shutdown send status and Clippy annotations was followed by
revalidation of the 25 all-features unit tests and Clippy.

Significant coverage: ordering and multi-producer bursts, snapshots, barriers
with active producers, shutdown races, control from a callback, a blocked
worker that can be released, timeouts including the admission lock, module
panics/errors, context exit, reentrant destruction outside locks, exhausted
IDs without panic, thread isolation and thresholds, lazy macros and borrowing,
log/tracing bridges, missing symbols, partial writes/errors, rotation and a
bounded cache, stdout errors, and invalid configuration.

The stripped-binary test does not guarantee a complete absence of resolved
symbols: empty/partial cases are also covered by unit fixtures. The tests do
not establish support for Windows/macOS/ARM, physical disk persistence, or
every possible interleaving of a concurrent program.

## Clippy and review

The historical public Level::to_string method returns the full name, while
Display returns a short code. A documented local
`inherent_to_string_shadow_display` exception preserves both behaviors. No
global Clippy suppression was added; the global allow(dead_code) was removed.
The remaining warnings mainly concern explicit returns, simplifiable if
expressions, historical Into conversions, &String arguments, redundant bounds,
and initializations. Historical tests also have naming/initialization warnings.
These results are not presented as a "zero warnings" validation and do not
justify a cosmetic overhaul.

The targeted review of transport, contexts, the cache, paths, macros, and
backtraces established no significant unresolved security/performance finding.
Risks involving destructors under locks were fixed and tested. Two semantic
limitations are accepted and documented in the README:

- Reentrant traces and context exits from the worker are rejected to prevent
  logging loops; blocking controls return Reentrant.
- Replacing the global context is not a barrier for snapshots still being
  constructed by other producers; pause producers and flush before
  reconfiguration when a strict boundary is required.

Debug/Display implementations and expressions in the calling code can panic;
their panics are not silently converted into logger success.

## Measurements

See the [protocol and comparison](README.md), [comparison data](comparison.json),
and [file output study](file_output.md). 30 valid before runs + 30 valid after
runs, with no detected loss or timeouts. The harness was identical, external
dependencies were unchanged, and sources remained stable during measurements.
The dates, uncontrolled load, small volumes, and sampling limit the conclusions.

## API and README review — 2026-11-04

HTraceError now borrows in all forms, with no marker or consuming variant.
HTracer::drop has been removed; examples and tests call shutdown. The macro
test checks reuse of owned Result Ok/Err values, borrowed and temporary errors,
and single evaluation even when the level is filtered out.

Updated validation: 37 all-features tests (25 unit tests, 9 integration tests,
3 doctests), 28 with no default features; the README example was extracted,
compiled, and run successfully. Clippy passes with 49 library warnings;
targeted formatting and diff-check pass. The all-features total in the table
has been corrected using the aggregated Cargo output.

The measurements from November 3–4 and their fingerprints remain dated results
from before this API revision; they have not been rewritten. The current
harness calls shutdown directly and checks its result.

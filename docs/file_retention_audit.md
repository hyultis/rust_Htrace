# File cleanup confinement audit

Audit date: 2026-11-05. Scope: the destructive path in `File` cleanup, its
configuration boundary, root resolution, candidate selection, cache locking,
and platform-specific path semantics. No user files were used as test targets.

## Conclusion

**The implementation cannot provide unconditional protection against deletion
outside the trace directory when another actor can replace that directory or
its ancestors.** A deterministic test against an instrumented copy of the actual
implementation reproduces such a deletion after the final root check.

Under the supported contract of a trusted, stable directory namespace, the
algorithm limits deletion to direct regular `.trc` entries of the resolved
configured directory. The audit found Windows path-spelling ambiguities and
hardened the implementation to use a single canonical namespace throughout.
Windows runtime behavior still requires native validation.

## Invariants checked

| Step | Invariant and result |
| --- | --- |
| Construction | Relative paths become absolute once, so a later working-directory change cannot redirect purge. Preserve the absolute spelling, including trailing separators. |
| Root validation | Reject observed symlinks/reparse points, non-directories, pathname roots (such as `/` or a drive/share root), and raw/stripped spellings that canonicalize differently. |
| Enumeration | Enumerate the saved canonical root only; never recurse and never enumerate `forceInOneFile` or source/thread-derived destinations. |
| Candidate selection | Require exact `.trc`, exactly one normal component, and raw basename equality. Reject absolute paths, prefixes, `..`, nested and normalized separator/dot spellings. |
| Candidate resolution | Join the basename to the saved canonical root; require the expected direct parent and exact canonical equality. Check regular-file type without following the final link; reject Windows reparse points. |
| Cache and age | Coordinate closing cached handles and deletion under the same writer mutex. Recheck type, identity hints, size, mtime and age; protect current destinations. |
| Removal | Recheck the root, then call `remove_file` on the canonical candidate. This last pair is **not atomic**. |

Conditional argument: in a stable namespace, a validated basename cannot add a
parent step, absolute root or extra component. Joining it to the canonical root
selects exactly one direct child. Selection never comes from an arbitrary output
path. A regular non-reparse child has no final-link traversal, and the only
destructive operation removes that single directory entry. No recursive deletion
exists in the production purge code.

“Inside” means the resolved configured directory, not a textual prefix. Existing
ancestor aliases can select another physical location; the application owns its
configuration. A hard link inside the directory is eligible: unlinking that local
name preserves outside aliases and their contents, although shared filesystem
metadata such as link count can change. Identity checks are hints, not proof of
immutability: Windows creation time is not a unique file identifier, and Unix
device/inode identifiers can be reused after deletion.

## Hardening performed

The initial implementation checked a normalized root but enumerated and removed
using its original path namespace. Windows can interpret terminal spaces/dots
differently once a child component is appended. Reserved DOS-like names also
have special meaning in ordinary Win32 paths. These were identified from
[Microsoft path-normalization rules](https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats)
and [Windows naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file),
not reproduced on a Windows machine in this environment.

The code now preserves the original absolute spelling, rejects ambiguous root
resolutions, and uses the saved canonical root for enumeration, candidate checks
and deletion. Rust documents that [canonical Windows paths use extended-length
syntax](https://doc.rust-lang.org/std/fs/fn.canonicalize.html). Raw malformed
basenames and pathname roots (such as `/` or a drive/share root) are also rejected. These changes do not turn
pathname operations into handle-relative operations.

## Executed adversarial tests

The audit suite includes malformed direct calls, absolute/parent/multiple
components, similarly prefixed sibling directories, nested `.trc` directories,
native names and exact extensions, literal percent-encoded separators, external
file/directory links, root links with slash/dot suffixes, hard links, external
`forceInOneFile` destinations, root/ancestor replacement before the final check,
and working-directory changes isolated in a child process. External fixture
sentinels are checked for both existence and unchanged contents. Pathname-root
refusal is tested through the read-only validator, never by purging a real root.

Windows-only cases cover malformed native names, unpaired UTF-16 surrogates,
ambiguous trailing-space roots and literal reserved basenames. They are not
executed on the Linux host; unsupported special-name creation reports an explicit
skip diagnostic. Linux tests and Android cross-checks do not establish Windows
runtime correctness.

## Reproduced residual race

Run on Unix with cached Cargo dependencies:

```sh
python3 tests/support/reproduce_retention_root_swap.py
```

The script copies the real sources to a temporary directory and inserts a
test-only one-shot action precisely between the final root check and deletion.
The action moves the root and replaces its old pathname with a symlink to a
sibling fixture containing an identically named sentinel. All ordinary
production checks have already succeeded. The real startup purge then deletes
the sibling sentinel while the moved original file remains.

This passed against the hardened implementation: `purgedFiles == 1` and
`purgeErrors == 0`. Here a passing reproduction means **the counterexample was
observed**, not that the algorithm is race-safe. The injection makes a possible
external interleaving deterministic; it is not present in the actual library.
Copied sources, build outputs and all fixtures are confined to temporary storage.

Rust explicitly documents [filesystem check/use races](https://doc.rust-lang.org/std/fs/index.html#time-of-check-to-time-of-use-toctou).
Another path check, a mutex owned only by HTrace, or immutable configuration
cannot eliminate this interval. A strict guarantee requires directory-handle
relative operations with appropriate platform semantics and a definition of
the authorized directory object even if it is renamed. That is a further design
change; this audit does not silently add a dependency or claim that std stable
already supplies that guarantee.

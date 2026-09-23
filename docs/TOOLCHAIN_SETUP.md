# Anchor/Solana Toolchain Setup and Troubleshooting

This guide is for future contributors, agents, and students extending this
repository into larger programs such as NFT marketplaces, staking programs, or
Token-2022 applications. It records the compatibility decisions that made this
AMM build reproducible on the legacy macOS/Solana installation used for the
assignment.

## Known-good baseline for this repository

Keep these components on one dependency family:

| Component | Version/family |
| --- | --- |
| Anchor CLI | 0.30.1 |
| `anchor-lang`, `anchor-spl` | 0.30.1 |
| Solana program crates | 1.18.26 |
| Solana CLI/SBF builder | 1.18.x |
| Rust used by SBF | approximately 1.75 |
| Cargo lockfile | version 3 |
| Rust edition | 2021 |

The host Rust installation is not authoritative for `anchor build`: Anchor
invokes the compiler and Cargo bundled with the Solana SBF toolchain. A
new-looking `rustc --version` therefore does not prove that the on-chain build
can compile newer crates.

## First-time setup

1. Install Node.js, Rust, Solana CLI, AVM, and the project dependencies.
2. Select the repository's Anchor version before running any command:

   ```bash
   avm use 0.30.1
   ~/.avm/bin/anchor-0.30.1 --version
   solana --version
   rustc --version
   cargo --version
   ```

   If the `anchor` wrapper says that the globally installed version is wrong,
   invoke the AVM binary directly as shown above. This avoids an architecture
   or PATH mismatch in the npm wrapper.
3. Install JavaScript dependencies and run checks in this order:

   ```bash
   npm install
   cargo fmt --all -- --check
   cargo test --workspace
   ~/.avm/bin/anchor-0.30.1 build
   ~/.avm/bin/anchor-0.30.1 test
   ```

4. Do not delete or regenerate `Cargo.lock` casually. First inspect the Cargo
   version that the SBF builder uses. A newer host Cargo can silently upgrade
   the lockfile to version 4, which Cargo 1.75 cannot read.

## A deterministic diagnosis loop

When a build fails, capture versions and classify the first error rather than
fixing the last line of a long log:

```bash
avm list
solana --version
rustc --version
cargo --version
grep -n '^version' Cargo.lock | head
~/.avm/bin/anchor-0.30.1 build 2>&1 | tee /tmp/anchor-build.log
```

Then use the smallest repair:

| First error | Likely cause | Repair |
| --- | --- | --- |
| `lock file version 4 requires -Znext-lockfile-bump` | SBF Cargo is older than host Cargo | Restore lockfile version 3; do not run a newer Cargo against it. |
| `requires rustc 1.79/1.80/1.89` | A transitive crate escaped the legacy graph | Pin that crate to a version supported by the SBF compiler, then run `cargo update -p name --precise version` with compatible Cargo. |
| `edition2024 is unstable` | A new transitive dependency entered the graph | Pin or downgrade the dependency; do not change the whole project to edition 2024. |
| Anchor CLI version mismatch | `anchor` resolves to a different AVM/npm binary | Use `avm use 0.30.1` or the direct `~/.avm/bin/anchor-0.30.1` path. |
| `IDL doesn't exist` | IDL generation was disabled or failed earlier | Keep `idl-build` enabled and fix the earlier compiler error. |
| `Span::source_file` missing in `anchor-syn` | Anchor 0.30's optional external-alias IDL path is incompatible with the old proc-macro2 API | Use the checked-in `vendor/anchor-syn` patch, which disables only that optional path. |
| Hundreds of `MontFp! could not parse` errors | Global `procmacro2_semver_exempt` flags leaked into unrelated proc macros | Remove global `RUSTFLAGS`/`.cargo/config.toml` flags and rebuild. |
| SBF stack offset exceeds 4096 | An instruction account context is too large | Box large account fields, split contexts, or reduce temporary locals; do not treat this warning as harmless for production. |

After changing dependency versions, inspect the graph before retrying:

```bash
cargo tree -i problematic-crate
cargo tree -d
```

Make one related change at a time and rerun `cargo test --workspace` before
repeating the full SBF build. This prevents lockfile churn from hiding the
actual cause.

## IDL and account/API discipline

IDL failures are often symptoms of a toolchain mismatch, not an application
bug. Keep the program's `idl-build` feature enabled:

```toml
idl-build = ["anchor-lang/idl-build", "anchor-spl/idl-build"]
```

After a successful build, verify that `target/idl/<program>.json` exists and
that its instruction/account names match the TypeScript tests. When adding
NFT metadata, associated-token accounts, or Token-2022 extensions, add the
corresponding IDL/build feature and test one instruction at a time before
adding the next account constraint.

For larger projects:

- Pin Anchor, Solana crates, SPL crates, and the lockfile together.
- Prefer a fresh workspace over copying a stale `Cargo.lock` from another
  Anchor project.
- Keep `anchor-lang` and `anchor-spl` on the same minor version.
- Treat Token-2022 as a deliberate dependency choice: verify the mint,
  extension, token program, and account owner in constraints; do not assume
  every SPL account uses the legacy token program.
- Generate and inspect the IDL immediately after changing account structs,
  instruction arguments, or features.
- Keep pure math and serialization tests in Rust so they can run without a
  validator; reserve integration tests for CPI, PDA, token, and authorization
  behavior.

## Evidence and reproducibility

For a submission or CI artifact, save the exact command and output:

```bash
mkdir -p docs
~/.avm/bin/anchor-0.30.1 test 2>&1 | tee docs/test-output.txt
```

Record the tool versions beside the output. A terminal screenshot is useful
for homework evidence, but the text log is the durable, searchable artifact.
Do not claim an integration test passed based only on `cargo test`; the Anchor
test command must complete with exit code 0.

## Resources

These are the primary references used to resolve this project:

- [Anchor documentation](https://www.anchor-lang.com/docs)
- [Anchor GitHub repository and release history](https://github.com/coral-xyz/anchor)
- [Solana program development documentation](https://solana.com/docs/programs)
- [Solana account model](https://solana.com/docs/core/accounts)
- [SPL Token documentation](https://spl.solana.com/token)
- [Token-2022 extensions](https://solana.com/docs/tokens/extensions)
- [Cargo dependency specification](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [Cargo lockfile format](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
- [Anchor account constraints](https://www.anchor-lang.com/docs/references/account-constraints)
- [Anchor testing documentation](https://www.anchor-lang.com/docs/testing)

The repository's `Cargo.toml`, `Cargo.lock`, `Anchor.toml`, and vendored
`anchor-syn` patch are the executable record of the final compatibility
solution. Update this guide whenever the Anchor/Solana baseline changes.

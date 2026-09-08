# Project rules

## Language

- Write all project content in English: documentation, code identifiers, comments,
  CLI help, logs, errors, tests, scripts, and workflow names.
- Write commit messages, pull request titles and descriptions, and release notes
  in English.
- Preserve proper names, external API identifiers, and literal test fixtures when
  translation would change their meaning or behavior.
- User conversations may follow the user's preferred language.

## Validation and releases

- Run `cargo fmt --check`, `cargo test --locked`, and
  `cargo clippy --locked --all-targets -- -D warnings` before submitting changes.
- Keep the release workflow running on every push to `main`, without path filters
  or cancellation of earlier builds.
- Publish downloadable CLI archives for Linux x86_64, macOS Intel and Apple
  Silicon, and Windows x86_64, with SHA-256 checksums, after validation succeeds.
- Each release must point to the exact source commit and report its release
  version through `aditor --version`. Keep releases reproducible on workflow reruns.
- Do not commit automated version bumps to `main`; they would trigger more releases.
- Keep installation and release documentation in sync with the workflow.

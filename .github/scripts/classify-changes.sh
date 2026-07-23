#!/bin/sh

classify() {
    code=false
    docs=false

    while IFS= read -r path; do
        case "$path" in
            .github/workflows/ci.yml | \
            .github/workflows/publish-docs.yml | \
            Cargo.lock | Cargo.toml | CHANGELOG.md | CONTRIBUTING.md | \
            LICENSE | Makefile | README.md | SECURITY.md | \
            crates/h11r-python/Cargo.toml | \
            crates/h11r-python/benchmarks/render_results.py | \
            crates/h11r-python/pyproject.toml | \
            crates/h11r-python/python/h11r/* | \
            crates/h11r-python/src/* | \
            docs/assets/* | docs/overrides/* | docs/site/* | \
            examples/* | pyproject.toml | uv.lock | \
            zensical.toml)
                docs=true
                ;;
        esac

        case "$path" in
            .agents/* | .github/ISSUE_TEMPLATE/* | docs/* | \
            .github/workflows/publish-docs.yml | \
            zensical.toml | *.md | LICENSE*)
                ;;
            *)
                code=true
                ;;
        esac
    done

    printf 'code=%s\ndocs=%s\n' "$code" "$docs"
}

if [ "${1-}" = "--self-test" ]; then
    test "$(printf '%s\n' README.md | classify)" = "code=false
docs=true"
    test "$(printf '%s\n' .github/ISSUE_TEMPLATE/01-bug-report.yml | classify)" = "code=false
docs=false"
    test "$(printf '%s\n' docs/knowledge/tooling/zensical.md | classify)" = "code=false
docs=false"
    test "$(printf '%s\n' crates/h11r/src/lib.rs | classify)" = "code=true
docs=false"
    test "$(printf '%s\n' Cargo.toml | classify)" = "code=true
docs=true"
    test "$(printf '%s\n' README.md crates/h11r/src/lib.rs | classify)" = "code=true
docs=true"
    test "$(printf '%s\n' examples/python/round_trip.py | classify)" = "code=true
docs=true"
    exit
fi

classify

const WORKFLOW: &str = include_str!("../.github/workflows/security.yml");
const DEPENDABOT: &str = include_str!("../.github/dependabot.yml");
const MIRI_RUNNER: &str = include_str!("../scripts/run-miri-shard.mjs");

#[test]
fn security_jobs_are_scheduled_bounded_and_failing() {
    assert!(WORKFLOW.contains("cron: '23 4 * * 1'"));
    assert!(WORKFLOW.contains("workflow_dispatch:"));
    assert!(WORKFLOW.contains("pull_request:"));
    assert!(WORKFLOW.contains("- Cargo.lock"));
    assert_eq!(WORKFLOW.matches("timeout-minutes:").count(), 4);
    assert!(!WORKFLOW.contains("continue-on-error"));
}

#[test]
fn audit_miri_and_sanitizer_commands_are_pinned() {
    assert!(WORKFLOW.contains("cargo-audit --version 0.22.2 --locked"));
    assert!(WORKFLOW.contains("audit --deny warnings"));
    assert!(WORKFLOW.contains("node scripts/run-miri-shard.mjs ${{ matrix.shard }} 16"));
    assert!(MIRI_RUNNER.contains("'+nightly-2026-10-20', 'miri', 'test', '--locked', '--lib'"));
    assert!(MIRI_RUNNER.contains("'--exact', '--nocapture'"));
    assert!(MIRI_RUNNER.contains("1 passed; 0 failed; 0 ignored;"));
    assert!(WORKFLOW.contains("needs: miri-shards"));
    assert!(WORKFLOW.contains("run: test \"$MIRI_RESULT\" = success"));
    assert!(WORKFLOW.contains("-Zsanitizer=address"));
    assert!(WORKFLOW.contains("halt_on_error=1:detect_leaks=0"));
}

#[test]
fn dependency_update_ecosystems_are_both_monitored() {
    assert!(DEPENDABOT.contains("package-ecosystem: cargo"));
    assert!(DEPENDABOT.contains("package-ecosystem: github-actions"));
    assert_eq!(DEPENDABOT.matches("interval: weekly").count(), 2);
}

#[test]
fn checkout_is_pinned_and_credentials_are_not_persisted() {
    let checkout_revisions = WORKFLOW.lines().filter_map(|line| {
        line.trim()
            .strip_prefix("uses: actions/checkout@")
            .and_then(|revision| revision.split_whitespace().next())
    });
    let checkout_revisions: Vec<_> = checkout_revisions.collect();

    assert!(!checkout_revisions.is_empty());
    assert!(
        checkout_revisions
            .iter()
            .all(|revision| revision.len() == 40
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit()))
    );
    assert!(
        checkout_revisions
            .windows(2)
            .all(|revisions| revisions[0] == revisions[1])
    );
    assert_eq!(
        WORKFLOW.matches("persist-credentials: false").count(),
        checkout_revisions.len()
    );
}

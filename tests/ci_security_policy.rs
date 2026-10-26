const WORKFLOW: &str = include_str!("../.github/workflows/security.yml");
const DEPENDABOT: &str = include_str!("../.github/dependabot.yml");

#[test]
fn security_jobs_are_scheduled_bounded_and_failing() {
    assert!(WORKFLOW.contains("cron: '23 4 * * 1'"));
    assert!(WORKFLOW.contains("workflow_dispatch:"));
    assert_eq!(WORKFLOW.matches("timeout-minutes:").count(), 3);
    assert!(!WORKFLOW.contains("continue-on-error"));
}

#[test]
fn audit_miri_and_sanitizer_commands_are_pinned() {
    assert!(WORKFLOW.contains("cargo-audit --version 0.22.2 --locked"));
    assert!(WORKFLOW.contains("audit --deny warnings"));
    assert!(WORKFLOW.contains("nightly-2026-10-20 miri test --lib"));
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
    assert_eq!(WORKFLOW.matches("actions/checkout@11d5960").count(), 3);
    assert_eq!(WORKFLOW.matches("persist-credentials: false").count(), 3);
}

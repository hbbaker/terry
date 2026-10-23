use terry::approvals::*;

#[test]
fn should_read_pane_only_when_the_approval_says_so() {
    assert_eq!(approval_via(r#"{"via": "pane"}"#), ApprovalVia::Pane);
    assert_eq!(approval_via(r#"{"via": "prompt"}"#), ApprovalVia::Prompt);
    assert_eq!(approval_via("{}"), ApprovalVia::Prompt);
    assert_eq!(approval_via("not json"), ApprovalVia::Prompt);
}

#[test]
fn should_spot_the_approvals_folder_in_a_command() {
    assert!(names_approvals("echo x > ~/.claude/diffs/approvals/a.json"));
    assert!(names_approvals("ls ~/.claude/diffs//approvals"));
    assert!(!names_approvals("ls ~/.claude/diffs/calls"));
}

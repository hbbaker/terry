use std::path::Path;

use terry::lint::*;

#[test]
fn should_describe_a_problem_with_its_place_tool_and_code() {
    let problem = LintProblem {
        path: "app.py".to_owned(),
        line: 1,
        column: 8,
        tool: "ruff".to_owned(),
        code: "F401".to_owned(),
        message: "`os` imported but unused".to_owned(),
        blocking: true,
    };
    assert_eq!(
        problem.describe(),
        "app.py:1:8 [ruff F401] `os` imported but unused"
    );
}

#[test]
fn should_skip_eslint_messages_that_have_no_line() {
    let output = r#"[{"filePath": "/r/a.ts", "messages": [
        {"message": "File ignored", "severity": 1},
        {"line": 2, "column": 7, "ruleId": "no-unused-vars", "message": "x", "severity": 2}
    ]}]"#;
    let problems = parse_eslint_json(Path::new("/r"), output).expect("parses");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].describe(), "a.ts:2:7 [eslint no-unused-vars] x");
    assert!(problems[0].blocking);
}

#[test]
fn should_pick_files_by_suffix() {
    let files = vec![
        "a.py".to_owned(),
        "b.ts".to_owned(),
        "c.sh".to_owned(),
        "README".to_owned(),
    ];
    assert_eq!(with_suffix(&files, &PYTHON_SUFFIXES), vec!["a.py"]);
    assert_eq!(with_suffix(&files, &ESLINT_SUFFIXES), vec!["b.ts"]);
}

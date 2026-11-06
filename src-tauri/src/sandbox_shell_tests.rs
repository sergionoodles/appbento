use super::*;

fn sandbox_plan(arguments: &[&str]) -> LaunchPlan {
    LaunchPlan {
        environment: Default::default(),
        program: "/usr/bin/bwrap".into(),
        arguments: arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect(),
        clear_environment: true,
        application_destination: None,
    }
}

#[test]
fn sandbox_command_puts_each_option_on_its_own_line() {
    let plan = sandbox_plan(&[
        "--unshare-net",
        "--ro-bind",
        "/usr",
        "/usr",
        "--setenv",
        "NAME",
        "two words",
        "--",
        "/app/tool",
        "--flag",
    ]);
    assert_eq!(
        render(&plan),
        "/usr/bin/bwrap \\\n  --unshare-net \\\n  --ro-bind /usr /usr \\\n  --setenv NAME 'two words' \\\n  -- /app/tool --flag"
    );
}

#[test]
fn quotes_single_quotes_and_empty_values() {
    let plan = sandbox_plan(&[
        "--setenv", "EMPTY", "", "--setenv", "QUOTE", "it's", "--", "/app/x",
    ]);
    let rendered = render(&plan);
    assert!(rendered.contains("--setenv EMPTY ''"));
    assert!(rendered.contains(r"--setenv QUOTE 'it'\''s'"));
}

#[test]
fn direct_command_stays_on_one_line_with_its_environment() {
    let plan = LaunchPlan {
        environment: [("MODE".to_owned(), "a b".to_owned())].into(),
        program: "/opt/My App/tool".into(),
        arguments: vec!["--verbose".into()],
        clear_environment: false,
        application_destination: None,
    };
    assert_eq!(render(&plan), "env MODE='a b' '/opt/My App/tool' --verbose");
}

use super::*;

fn sandbox_plan(arguments: &[&str]) -> LaunchPlan {
    LaunchPlan {
        identity: None,
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
        identity: None,
        environment: [("MODE".to_owned(), "a b".to_owned())].into(),
        program: "/opt/My App/tool".into(),
        arguments: vec!["--verbose".into()],
        clear_environment: false,
        application_destination: None,
    };
    assert_eq!(render(&plan), "env MODE='a b' '/opt/My App/tool' --verbose");
}

#[test]
fn actual_manual_shell_provisions_accounts_and_preserves_literal_values() {
    if !super::super::status().available {
        return;
    }
    let (directory, mut app) = super::super::tests::fixture();
    let literal = "spaces ' quotes $(exit 31) `exit 32` ; --";
    app.launch_config = Some(LaunchConfig {
        mode: crate::models::LaunchMode::Bubblewrap,
        arguments: vec![],
        environment: [("LITERAL".into(), literal.into())].into(),
        permissions: crate::models::SandboxPermissions {
            persistent_data: false,
            ..Default::default()
        },
    });
    let runtime = Runtime {
        identity: super::super::Identity {
            uid: 23456,
            gid: 34567,
        },
        ..Default::default()
    };
    let mut planned = plan(&app, &[], &directory.0.join("policy"), &runtime).unwrap();
    let index = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    planned.arguments.truncate(index + 1);
    planned.arguments.extend([
        "/bin/sh".into(), "-c".into(),
        "test \"$LITERAL\" = \"$1\" && test \"$(getent group 34567)\" = 'app:x:34567:' && exec /usr/bin/getent passwd 23456".into(),
        "account-check".into(), literal.into(),
    ]);
    let rendered = render(&planned);
    assert!(rendered.starts_with("sh -c "));
    for shell in ["/bin/sh", "/usr/bin/fish"] {
        if !Path::new(shell).exists() {
            continue;
        }
        let mut command = std::process::Command::new(shell);
        if shell.ends_with("/fish") {
            command.arg("--no-config");
        }
        let output = command.args(["-c", &rendered]).output().unwrap();
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"app:x:23456:34567::/home/app:/bin/sh\n");
    }
}

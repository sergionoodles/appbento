//! Renders a launch plan as a shell command people can run themselves.
use super::{plan, LaunchPlan, Runtime};
use crate::models::{DesktopIntegration, InstalledApp, LaunchConfig};
use std::path::Path;

const LINE_CONTINUATION: &str = " \\\n  ";
const ARGUMENT_SEPARATOR: &str = "--";

/// Builds the command for `config` without saving it. Desktop integration is left out because
/// its filtered D-Bus proxy only runs while AppBento supervises the launch.
pub fn manual_command(
    app: &InstalledApp,
    config: &LaunchConfig,
    policy_directory: &Path,
    runtime: &Runtime,
) -> Result<String, String> {
    let mut config = config.clone();
    config.permissions.desktop_integration = DesktopIntegration::default();
    let app = InstalledApp {
        launch_config: Some(config),
        ..app.clone()
    };
    let plan = plan(&app, &[], policy_directory, runtime)?;
    Ok(render(&plan))
}

/// Sandbox commands put each Bubblewrap option on its own line; the app command stays on one.
pub(super) fn render(plan: &LaunchPlan) -> String {
    let environment = plan
        .environment
        .iter()
        .map(|(name, value)| format!("{name}={}", quote(value)));
    let prefix = if plan.environment.is_empty() {
        Vec::new()
    } else {
        std::iter::once("env".to_owned())
            .chain(environment)
            .collect()
    };
    let program = prefix
        .into_iter()
        .chain(std::iter::once(quote(&plan.program)))
        .collect::<Vec<_>>()
        .join(" ");
    if !plan.clear_environment {
        return std::iter::once(program)
            .chain(plan.arguments.iter().map(|argument| quote(argument)))
            .collect::<Vec<_>>()
            .join(" ");
    }
    let Some(identity) = plan.identity else {
        return sandbox_lines(program, &plan.arguments).join(LINE_CONTINUATION);
    };
    let [(passwd, _), (group, _)] = identity.contents();
    // These descriptors exist only in this shell invocation and are consumed by bwrap.
    let arguments = [
        "--ro-bind-data",
        "3",
        "/etc/passwd",
        "--ro-bind-data",
        "4",
        "/etc/group",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(plan.arguments.iter().cloned())
    .collect::<Vec<_>>();
    let command = sandbox_lines(program, &arguments).join(LINE_CONTINUATION);
    let script = format!("{command} 3<<'APPBENTO_PASSWD' 4<<'APPBENTO_GROUP'\n{passwd}APPBENTO_PASSWD\n{group}APPBENTO_GROUP\n");
    format!("sh -c {}", quote(&script))
}

fn sandbox_lines(program: String, arguments: &[String]) -> Vec<String> {
    let separator = arguments
        .iter()
        .position(|argument| argument == ARGUMENT_SEPARATOR)
        .unwrap_or(arguments.len());
    let (options, command) = arguments.split_at(separator);
    let mut lines = vec![program];
    for option in options {
        let quoted = quote(option);
        match lines.last_mut() {
            Some(line) if !option.starts_with("--") => {
                line.push(' ');
                line.push_str(&quoted);
            }
            _ => lines.push(quoted),
        }
    }
    if !command.is_empty() {
        let command = command.iter().map(|argument| quote(argument));
        lines.push(command.collect::<Vec<_>>().join(" "));
    }
    lines
}

fn quote(value: &str) -> String {
    let is_safe = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-./:=@%+,".contains(character));
    if is_safe {
        return value.to_owned();
    }
    // Fish interprets backslashes inside single quotes, unlike POSIX sh. Escape
    // both quotes and backslashes outside the quoted spans so either shell works.
    let mut quoted = String::from("'");
    for character in value.chars() {
        match character {
            '\'' => quoted.push_str(r"'\''"),
            '\\' => quoted.push_str(r"'\\'"),
            _ => quoted.push(character),
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
#[path = "sandbox_shell_tests.rs"]
mod tests;

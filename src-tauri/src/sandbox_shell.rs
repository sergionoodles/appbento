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
    sandbox_lines(program, &plan.arguments).join(LINE_CONTINUATION)
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
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "sandbox_shell_tests.rs"]
mod tests;

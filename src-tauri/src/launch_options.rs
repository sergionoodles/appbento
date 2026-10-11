use crate::models::LaunchConfig;

pub(super) fn validate(config: &LaunchConfig) -> Result<(), String> {
    if config
        .arguments
        .iter()
        .any(|argument| argument.contains('\0'))
    {
        return Err("launch arguments must not contain null characters".into());
    }
    for (name, value) in &config.environment {
        let mut characters = name.chars();
        let valid_start = characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
        if !valid_start
            || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err("environment variable names must start with a letter or underscore and contain only letters, digits, or underscores".into());
        }
        if value.contains('\0') {
            return Err("environment variable values must not contain null characters".into());
        }
    }
    Ok(())
}

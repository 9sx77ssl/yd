use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleId {
    Wallet,
}

#[derive(Clone, Copy, Debug)]
pub struct ModuleSpec {
    pub id: ModuleId,
    pub command: &'static str,
    pub long_alias: &'static str,
    pub summary: &'static str,
}

pub const MODULES: &[ModuleSpec] = &[ModuleSpec {
    id: ModuleId::Wallet,
    command: "wallet",
    long_alias: "--wallet",
    summary: "Show balances and wallet controls",
}];

impl ModuleSpec {
    pub fn short_alias(self) -> String {
        let first = self
            .command
            .chars()
            .next()
            .expect("module commands must not be empty");
        format!("-{first}")
    }

    pub fn matches_alias(self, argument: &OsString) -> bool {
        argument == self.short_alias().as_str() || argument == self.long_alias
    }
}

pub fn normalize_arguments(mut arguments: Vec<OsString>) -> Vec<OsString> {
    for argument in arguments.iter_mut().skip(1) {
        if let Some(module) = MODULES.iter().find(|module| module.matches_alias(argument)) {
            *argument = OsString::from(module.command);
            break;
        }
    }
    arguments
}

pub fn root_help() -> String {
    let rows = MODULES
        .iter()
        .map(|module| {
            format!(
                "  {}, {}  {}",
                module.short_alias(),
                module.long_alias,
                module.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    "Modules:\n".to_owned() + &rows + "\n\nUse `yd <module alias> -h` for module options."
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn normalizes_registered_aliases_once() {
        let arguments = vec!["yd", "-w", "-h"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let normalized = normalize_arguments(arguments);
        assert_eq!(normalized, vec!["yd", "wallet", "-h"]);
    }

    #[test]
    fn leaves_unknown_arguments_untouched() {
        let arguments = vec!["yd", "-r"].into_iter().map(OsString::from).collect();
        assert_eq!(normalize_arguments(arguments), vec!["yd", "-r"]);
    }

    #[test]
    fn generated_short_aliases_are_unique() {
        let mut aliases = HashSet::new();
        for module in MODULES {
            assert!(
                aliases.insert(module.short_alias()),
                "duplicate module short alias for {}",
                module.command
            );
        }
    }
}

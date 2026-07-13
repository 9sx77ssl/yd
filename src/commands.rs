use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleId {
    Wallet,
}

#[derive(Clone, Copy, Debug)]
pub struct ModuleSpec {
    pub id: ModuleId,
    pub command: &'static str,
    pub short_alias: &'static str,
    pub long_alias: &'static str,
    pub summary: &'static str,
}

pub const MODULES: &[ModuleSpec] = &[ModuleSpec {
    id: ModuleId::Wallet,
    command: "wallet",
    short_alias: "-w",
    long_alias: "--wallet",
    summary: "Show balances and wallet controls",
}];

pub fn normalize_arguments(mut arguments: Vec<OsString>) -> Vec<OsString> {
    for argument in arguments.iter_mut().skip(1) {
        if let Some(module) = MODULES
            .iter()
            .find(|module| argument == module.short_alias || argument == module.long_alias)
        {
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
                module.short_alias, module.long_alias, module.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    "Modules:\n".to_owned() + &rows + "\n\nUse `yd <module alias> -h` for module options."
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

use super::*;

#[cfg(test)]
pub(super) fn actor_registry_error(error: VmProcessRegistryError) -> String {
    match error {
        VmProcessRegistryError::EmptyName => "actor name cannot be empty".to_string(),
        VmProcessRegistryError::NameNotRegistered(name) => {
            format!("actor name `{name}` is not registered")
        }
        VmProcessRegistryError::MissingProcess(pid) => {
            format!("cannot register missing process {}", pid.as_u64())
        }
        VmProcessRegistryError::ExitedProcess(pid) => {
            format!("cannot register exited process {}", pid.as_u64())
        }
        VmProcessRegistryError::Conflict { name, existing } => format!(
            "actor name `{name}` is already registered to process {}",
            existing.as_u64()
        ),
    }
}

#[cfg(test)]
pub(super) fn actor_alias_error(error: VmProcessAliasError) -> String {
    match error {
        VmProcessAliasError::MissingProcess(pid) => {
            format!("cannot alias missing process {}", pid.as_u64())
        }
        VmProcessAliasError::ExitedProcess(pid) => {
            format!("cannot alias exited process {}", pid.as_u64())
        }
        VmProcessAliasError::MissingAlias(alias) => {
            format!("process alias {} is not registered", alias.as_u64())
        }
        VmProcessAliasError::PriorityNotEnabled(alias) => {
            format!("process alias {} is not priority-enabled", alias.as_u64())
        }
        VmProcessAliasError::AliasSpaceExhausted => {
            "process alias identity space is exhausted".to_string()
        }
    }
}

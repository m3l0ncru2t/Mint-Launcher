use super::manifest::{Rule, RuleAction};
use std::collections::HashMap;

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "unknown"
    }
}

/// Mojang's rule evaluation: with no rules, the item is included. With rules
/// present, each rule is evaluated in order; the action of the last matching
/// rule wins. A rule matches when every constraint it specifies (os name/arch,
/// feature flags) is satisfied.
pub fn rules_allow(rules: Option<&[Rule]>, active_features: &HashMap<String, bool>) -> bool {
    let rules = match rules {
        None => return true,
        Some(r) => r,
    };
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, active_features) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, active_features: &HashMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            if name != current_os_name() {
                return false;
            }
        }
        if let Some(arch) = &os.arch {
            if arch != current_arch() {
                return false;
            }
        }
    }
    if let Some(features) = &rule.features {
        for (key, expected) in features {
            let actual = active_features.get(key).copied().unwrap_or(false);
            if actual != *expected {
                return false;
            }
        }
    }
    true
}

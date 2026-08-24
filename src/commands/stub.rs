//! Hidden 1.0 stubs for dropped top-level verbs. Exit 2. Do not run the old work.

use anyhow::Result;

use crate::error::RemovedVerb;

pub fn run(verb: &'static str, rest: &[String]) -> Result<()> {
    let token = first_name(rest);
    let mut msg = format!(
        "error: removed-in-1.0: {verb}\n`zskills {verb}` was removed in 1.0.\n{}",
        doors(verb)
    );
    if let Some(name) = token {
        if let Some(hint) = live_hint(name) {
            msg.push('\n');
            msg.push_str(&hint);
        }
    }
    Err(RemovedVerb { message: msg }.into())
}

fn first_name(rest: &[String]) -> Option<&str> {
    rest.iter()
        .map(String::as_str)
        .find(|a| !a.starts_with('-'))
}

fn doors(verb: &str) -> String {
    match verb {
        "install" => {
            "  plugin:      zskills plugin install <name@marketplace>  (writes [[skills]])\n  skill:       zskills skill install <owner/repo>            (writes [[agent_skills]])\n  mcp:         zskills mcp add <name>".into()
        }
        "remove" => {
            "  plugin:      zskills plugin remove <name@marketplace>\n  skill:       zskills skill remove <name>\n  mcp:         zskills mcp remove <name> [--scope user|project|local]".into()
        }
        "purge" => "  plugin:      zskills plugin purge <name@marketplace>".into(),
        "enable" => "  plugin:      zskills plugin enable <name@marketplace>".into(),
        "disable" => "  plugin:      zskills plugin disable <name@marketplace>".into(),
        "update" => "  marketplace: zskills marketplace update [name]".into(),
        "upgrade" => {
            "  skill:       zskills skill upgrade [name]\n  marketplace: zskills marketplace update [name]".into()
        }
        "skill-migrate" => {
            "  use `zskills migrate-skill <name>` (top-level). `skill migrate` is not a verb.".into()
        }
        _ => String::new(),
    }
}

fn live_hint(name: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Ok(settings) = crate::paths::settings_json().and_then(|p| crate::settings::load(&p)) {
        if let Some(ep) = crate::settings::enabled_plugins(&settings) {
            let hits: Vec<&String> = ep
                .keys()
                .filter(|k| *k == name || k.starts_with(&format!("{name}@")))
                .collect();
            for k in hits {
                lines.push(format!(
                    "  plugin      {k}    → zskills plugin remove {k}      (keeps bytes)"
                ));
            }
        }
    }
    if let Ok(inv) = crate::agent_skill::load_inventory() {
        if inv.agent_skills.contains_key(name) {
            lines.push(format!(
                "  skill       {name}                 → zskills skill remove {name} (DELETES bytes)"
            ));
        }
    }
    if let Ok(mcps) = crate::mcp::load_all() {
        for m in mcps.iter().filter(|m| m.name == name) {
            lines.push(format!(
                "  mcp         {name} ({})     → zskills mcp remove {name} --scope {}",
                m.scope.label(),
                m.scope.label()
            ));
        }
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "`{name}` currently exists in {} of your namespaces:\n{}",
        lines.len(),
        lines.join("\n")
    ))
}

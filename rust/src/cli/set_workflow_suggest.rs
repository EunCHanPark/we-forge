use super::*;

pub fn run(state: &str) -> Result<()> {
    let enable = match state {
        "on"  => true,
        "off" => false,
        _ => { eprintln!("  FAIL state must be 'on' or 'off'"); return Err(anyhow::anyhow!("bad state")); }
    };
    let mut cfg = config::with_env_overrides(config::load());
    let old = cfg.workflow_suggest_enabled;
    cfg.workflow_suggest_enabled = enable;
    config::save(&cfg)?;
    println!("  OK workflow-suggest: {} → {}",
             if old    { "on" } else { "off" },
             if enable { "on" } else { "off" });
    if enable {
        println!("  effect:   skill-suggest injections will now append ECC multi-agent");
        println!("            workflow recommendations (santa-method / council /");
        println!("            multi-workflow / gan-style-harness / …) when prompts");
        println!("            trigger their patterns.");
    } else {
        println!("  effect:   skill-suggest reverts to skill-only suggestions.");
    }
    println!("  config:   {}", paths::config_file().display());
    let _ = ecc_core::log("enterprise-agent-ops",
        &format!("workflow-suggest toggled to {}", if enable { "on" } else { "off" }),
        "cli");
    Ok(())
}

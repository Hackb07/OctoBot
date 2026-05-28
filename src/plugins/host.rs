use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::{
    models::{OpsEvent, PluginDescriptor, PluginKind, PluginStatus},
    security::{PluginSecurity, SecurityPolicy, redact_sensitive},
    utils::now_ts,
};

pub(crate) type PluginResult<T = ()> = Result<T, String>;

pub(crate) trait Plugin: Send + Sync {
    fn descriptor(&self) -> &PluginDescriptor;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn init(&mut self) -> PluginResult {
        Ok(())
    }
    fn start(&mut self) -> PluginResult {
        Ok(())
    }
    fn stop(&mut self) -> PluginResult {
        Ok(())
    }
    fn shutdown(&mut self) -> PluginResult {
        Ok(())
    }
    fn execute(&mut self, _input: &str) -> PluginResult<String> {
        Err("plugin does not support direct execution".into())
    }
}

pub(crate) struct ExternalScriptPlugin {
    descriptor: PluginDescriptor,
    script_path: String,
    running: bool,
}

impl ExternalScriptPlugin {
    pub(crate) fn new(descriptor: PluginDescriptor, script_path: String) -> Self {
        Self {
            descriptor,
            script_path,
            running: false,
        }
    }
}

impl Plugin for ExternalScriptPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn init(&mut self) -> PluginResult {
        let path = std::path::Path::new(&self.script_path);
        if !path.exists() {
            return Err(format!("script not found: {}", self.script_path));
        }
        PluginSecurity::validate_descriptor(&self.descriptor)?;
        PluginSecurity::validate_script_path(path)?;
        Ok(())
    }

    fn start(&mut self) -> PluginResult {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> PluginResult {
        self.running = false;
        Ok(())
    }

    fn execute(&mut self, input: &str) -> PluginResult<String> {
        if !self.running {
            return Err("plugin is not running".into());
        }
        PluginSecurity::enforce_runtime_boundaries(&self.descriptor, input)?;
        let output = std::process::Command::new(&self.script_path)
            .arg(SecurityPolicy::sanitize_prompt(input))
            .output()
            .map_err(|e| format!("execution failed: {e}"))?;
        let stdout = redact_sensitive(&String::from_utf8_lossy(&output.stdout))
            .trim()
            .to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = redact_sensitive(&String::from_utf8_lossy(&output.stderr))
                .trim()
                .to_string();
            Err(format!("script failed ({}): {stderr}", output.status))
        }
    }
}

pub(crate) struct NativePlugin {
    pub(crate) descriptor: PluginDescriptor,
    pub(crate) data: HashMap<String, String>,
    running: bool,
}

impl NativePlugin {
    pub(crate) fn new(
        name: &str,
        kind: PluginKind,
        description: &str,
        version: &str,
        owner: &str,
    ) -> Self {
        Self {
            descriptor: PluginDescriptor {
                name: name.into(),
                kind,
                description: description.into(),
                version: version.into(),
                status: PluginStatus::Registered,
                owner: owner.into(),
            },
            data: HashMap::new(),
            running: false,
        }
    }
}

impl Plugin for NativePlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn init(&mut self) -> PluginResult {
        PluginSecurity::validate_descriptor(&self.descriptor)?;
        self.descriptor.status = PluginStatus::Registered;
        Ok(())
    }

    fn start(&mut self) -> PluginResult {
        self.running = true;
        self.descriptor.status = PluginStatus::Enabled;
        Ok(())
    }

    fn stop(&mut self) -> PluginResult {
        self.running = false;
        self.descriptor.status = PluginStatus::Disabled;
        Ok(())
    }

    fn execute(&mut self, input: &str) -> PluginResult<String> {
        if !self.running {
            return Err("plugin is not enabled".into());
        }
        PluginSecurity::enforce_runtime_boundaries(&self.descriptor, input)?;
        let result = format!(
            "plugin {} processed: {}",
            self.descriptor.name,
            SecurityPolicy::sanitize_prompt(input)
        );
        Ok(result)
    }
}

pub(crate) struct PluginInstance {
    pub(crate) plugin: Box<dyn Plugin>,
    pub(crate) loaded_at: String,
}

impl PluginInstance {
    pub(crate) fn new(plugin: Box<dyn Plugin>) -> Self {
        Self {
            plugin,
            loaded_at: now_ts(),
        }
    }
}

/// Scan a directory for plugin manifests (JSON files with plugin descriptor).
pub(crate) fn discover_plugins(dir: &str) -> Vec<PluginDescriptor> {
    let dir_path = std::path::Path::new(dir);
    if !dir_path.is_dir() {
        return Vec::new();
    }
    let mut plugins = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(desc) = serde_json::from_str::<PluginDescriptor>(&content) {
                        if PluginSecurity::validate_descriptor(&desc).is_ok() {
                            plugins.push(desc);
                        }
                    }
                }
            }
        }
    }
    plugins
}

/// Load a plugin from a manifest directory entry.
pub(crate) fn load_plugin_from_dir(
    path: &std::path::Path,
    descriptor: &PluginDescriptor,
) -> Option<Box<dyn Plugin>> {
    let script_path = path.with_extension("sh");
    if PluginSecurity::validate_descriptor(descriptor).is_err() {
        return None;
    }
    if script_path.exists() {
        if PluginSecurity::validate_script_path(&script_path).is_err() {
            return None;
        }
        Some(Box::new(ExternalScriptPlugin::new(
            descriptor.clone(),
            script_path.to_string_lossy().to_string(),
        )))
    } else {
        let native = NativePlugin::new(
            &descriptor.name,
            descriptor.kind.clone(),
            &descriptor.description,
            &descriptor.version,
            &descriptor.owner,
        );
        Some(Box::new(native))
    }
}

/// Wire a plugin lifecycle event into the event bus.
pub(crate) fn emit_plugin_registered(
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
    descriptor: &PluginDescriptor,
) {
    let _ = event_tx.send(OpsEvent::PluginRegistered {
        plugin: descriptor.clone(),
    });
}

pub(crate) fn emit_plugin_status_changed(
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
    name: &str,
    status: PluginStatus,
) {
    let _ = event_tx.send(OpsEvent::PluginStatusChanged {
        name: name.into(),
        status,
        timestamp: now_ts(),
    });
}

/// Generate an SDK-level documentation snippet for the plugin manifest format.
pub(crate) fn manifest_doc() -> &'static str {
    r#"# OctoBot Plugin Manifest (JSON)

{
  "name": "my-plugin",
  "kind": "Tool",
  "description": "Does something useful",
  "version": "0.1.0",
  "owner": "operator"
}

# Plugin Kinds: Tool, Workflow, Integration, Agent
# Status lifecycle: Registered -> Enabled -> Disabled -> Registered
# Place .json manifest + optional .sh script in OCTOBOT_PLUGIN_DIR
# /plugin add <name> <kind>    -- register a new plugin
# /plugin enable <name>        -- enable a plugin
# /plugin disable <name>       -- disable a plugin
"#
}

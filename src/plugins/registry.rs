use std::collections::HashMap;
use std::path::Path;

use tokio::sync::mpsc;

use crate::{
    models::{OpsEvent, PluginDescriptor, PluginKind, PluginStatus},
    persistence::PersistenceRuntime,
    plugins::host::{
        Plugin, PluginInstance, discover_plugins, emit_plugin_registered,
        emit_plugin_status_changed, load_plugin_from_dir,
    },
    security::PluginSecurity,
};

pub(crate) struct PluginRegistry {
    plugins: HashMap<String, PluginInstance>,
    plugin_dir: Option<String>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    watcher_interval_secs: u64,
}

impl PluginRegistry {
    pub(crate) fn new(
        plugin_dir: Option<String>,
        event_tx: mpsc::UnboundedSender<OpsEvent>,
    ) -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_dir,
            event_tx,
            watcher_interval_secs: 30,
        }
    }

    pub(crate) fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<(), String> {
        let name = plugin.descriptor().name.clone();
        PluginSecurity::validate_descriptor(plugin.descriptor())?;
        if self.plugins.contains_key(&name) {
            return Err(format!("plugin '{name}' is already registered"));
        }
        let mut instance = PluginInstance::new(plugin);
        instance
            .plugin
            .init()
            .map_err(|e| format!("init failed: {e}"))?;
        emit_plugin_registered(&self.event_tx, instance.plugin.descriptor());
        self.plugins.insert(name, instance);
        Ok(())
    }

    pub(crate) fn enable(&mut self, name: &str) -> Result<(), String> {
        let instance = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| format!("plugin '{name}' not found"))?;
        instance.plugin.start()?;
        emit_plugin_status_changed(&self.event_tx, name, PluginStatus::Enabled);
        Ok(())
    }

    pub(crate) fn disable(&mut self, name: &str) -> Result<(), String> {
        let instance = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| format!("plugin '{name}' not found"))?;
        instance.plugin.stop()?;
        emit_plugin_status_changed(&self.event_tx, name, PluginStatus::Disabled);
        Ok(())
    }

    pub(crate) fn unregister(&mut self, name: &str) -> Result<PluginDescriptor, String> {
        let mut instance = self
            .plugins
            .remove(name)
            .ok_or_else(|| format!("plugin '{name}' not found"))?;
        instance.plugin.shutdown()?;
        let desc = instance.plugin.descriptor().clone();
        Ok(desc)
    }

    pub(crate) fn execute(&mut self, name: &str, input: &str) -> Result<String, String> {
        let instance = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| format!("plugin '{name}' not found"))?;
        instance.plugin.execute(input)
    }

    pub(crate) fn list(&self) -> Vec<PluginDescriptor> {
        self.plugins
            .values()
            .map(|instance| instance.plugin.descriptor().clone())
            .collect()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&PluginDescriptor> {
        self.plugins
            .get(name)
            .map(|instance| instance.plugin.descriptor())
    }

    pub(crate) fn count(&self) -> usize {
        self.plugins.len()
    }

    pub(crate) fn enabled_count(&self) -> usize {
        self.plugins
            .values()
            .filter(|i| i.plugin.descriptor().status == PluginStatus::Enabled)
            .count()
    }

    pub(crate) fn load_from_dir(&mut self) -> Vec<String> {
        let dir = match self.plugin_dir.clone() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let descriptors = discover_plugins(&dir);
        let mut loaded = Vec::new();
        for desc in descriptors {
            if self.plugins.contains_key(&desc.name) {
                continue;
            }
            let path = Path::new(&dir).join(&desc.name);
            if let Some(plugin) = load_plugin_from_dir(&path, &desc) {
                let name = plugin.descriptor().name.clone();
                match self.register(plugin) {
                    Ok(()) => loaded.push(name),
                    Err(e) => {
                        tracing::warn!(plugin = %desc.name, error = %e, "failed to register plugin from dir");
                    }
                }
            }
        }
        loaded
    }

    pub(crate) fn hot_reload(&mut self) -> Vec<String> {
        let loaded = self.load_from_dir();
        self.remove_stale_plugins();
        loaded
    }

    fn remove_stale_plugins(&mut self) {
        let Some(ref dir) = self.plugin_dir else {
            return;
        };
        let dir_path = Path::new(dir);
        if !dir_path.is_dir() {
            return;
        }
        let active: Vec<String> = std::fs::read_dir(dir_path)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.path().file_stem()?.to_str()?.to_string();
                Some(name)
            })
            .collect();

        let stale: Vec<String> = self
            .plugins
            .keys()
            .filter(|name| !active.contains(name))
            .cloned()
            .collect();

        for name in stale {
            let _ = self.unregister(&name);
            tracing::info!(plugin = %name, "removed stale plugin via hot-reload");
        }
    }

    pub(crate) fn watcher_interval_secs(&self) -> u64 {
        self.watcher_interval_secs
    }

    pub(crate) fn set_watcher_interval_secs(&mut self, secs: u64) {
        self.watcher_interval_secs = secs;
    }

    pub(crate) fn summary(&self) -> String {
        let total = self.count();
        let enabled = self.enabled_count();
        let kinds: Vec<String> = self
            .list()
            .iter()
            .map(|d| format!("{:?}", d.kind))
            .collect();
        format!("{total} plugins ({enabled} enabled): {}", kinds.join(", "))
    }
}

pub(crate) struct PluginApi;

impl PluginApi {
    pub(crate) async fn handle_command(
        registry: &mut PluginRegistry,
        command: &str,
        _persistence: Option<&PersistenceRuntime>,
    ) -> String {
        let parts: Vec<&str> = command.splitn(3, |c: char| c.is_whitespace()).collect();
        match parts.as_slice() {
            ["plugin", "add", rest] if !rest.is_empty() => {
                let mut sub = rest.splitn(2, |c: char| c.is_whitespace());
                let name = sub.next().unwrap_or("custom");
                let kind_str = sub.next().unwrap_or("tool");
                let kind = match kind_str.to_ascii_lowercase().as_str() {
                    "workflow" => PluginKind::Workflow,
                    "integration" => PluginKind::Integration,
                    "agent" => PluginKind::Agent,
                    _ => PluginKind::Tool,
                };
                let descriptor = PluginDescriptor {
                    name: name.into(),
                    kind,
                    description: format!("user-registered {kind_str} plugin"),
                    version: "0.1.0".into(),
                    status: PluginStatus::Registered,
                    owner: "operator".into(),
                };
                let scopes = PluginSecurity::permissions_for_kind(&descriptor.kind).join(",");
                let plugin = crate::plugins::host::NativePlugin::new(
                    &descriptor.name,
                    descriptor.kind.clone(),
                    &format!("{} [{}]", descriptor.description, scopes),
                    &descriptor.version,
                    &descriptor.owner,
                );
                match registry.register(Box::new(plugin)) {
                    Ok(()) => format!("plugin '{name}' registered"),
                    Err(e) => format!("error: {e}"),
                }
            }
            ["plugin", "enable", name] if !name.is_empty() => match registry.enable(name) {
                Ok(()) => format!("plugin '{name}' enabled"),
                Err(e) => format!("error: {e}"),
            },
            ["plugin", "disable", name] if !name.is_empty() => match registry.disable(name) {
                Ok(()) => format!("plugin '{name}' disabled"),
                Err(e) => format!("error: {e}"),
            },
            ["plugin", "remove", name] if !name.is_empty() => match registry.unregister(name) {
                Ok(desc) => format!("plugin '{}' removed", desc.name),
                Err(e) => format!("error: {e}"),
            },
            ["plugin", "list", ..] | ["plugin", "ls", ..] => {
                let descriptors = registry.list();
                if descriptors.is_empty() {
                    "no plugins registered".into()
                } else {
                    let mut lines = Vec::new();
                    for d in &descriptors {
                        lines.push(format!(
                            "  {:?} {:?} {} v{} ({})",
                            d.status, d.kind, d.name, d.version, d.owner
                        ));
                    }
                    lines.join("\n")
                }
            }
            ["plugin", "reload", ..] => {
                let loaded = registry.hot_reload();
                if loaded.is_empty() {
                    "hot-reload: no new plugins found".into()
                } else {
                    format!("hot-reload: loaded {}", loaded.join(", "))
                }
            }
            _ => "usage: plugin add|enable|disable|remove|list|reload [name] [kind]".into(),
        }
    }
}

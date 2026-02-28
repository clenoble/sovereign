use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store, StoreLimitsBuilder};

use sovereign_core::content::ContentFields;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};
use crate::wasm::host_bridge::{PluginState, SkillPlugin};
use crate::wasm::runner::WasmLimits;

/// A WASM skill that implements CoreSkill by delegating to a
/// wasmtime Component.
///
/// Each `execute()` call creates a fresh Store for full isolation.
/// Metadata (name, capabilities, actions, file_types) is cached
/// after loading to avoid repeated WASM instantiation.
pub struct WasmSkill {
    engine: Engine,
    component: Component,
    limits: WasmLimits,
    // Cached metadata
    cached_name: String,
    cached_capabilities: Vec<Capability>,
    cached_actions: Vec<(String, String)>,
    cached_file_types: Vec<String>,
}

impl WasmSkill {
    /// Load a WASM component and cache its metadata.
    pub(crate) fn new(
        engine: Engine,
        component: Component,
        limits: WasmLimits,
    ) -> anyhow::Result<Self> {
        // Instantiate once to read metadata exports, then discard the Store.
        let mut linker = Linker::new(&engine);
        SkillPlugin::add_to_linker::<PluginState, HasSelf<PluginState>>(
            &mut linker,
            |state| state,
        )?;

        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .instances(limits.max_instances)
            .build();
        let state = PluginState {
            db: None,
            limits: store_limits,
        };
        let mut store = Store::new(&engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(10_000_000)?; // Generous fuel for metadata calls

        let bindings = SkillPlugin::instantiate(&mut store, &component, &linker)?;

        let name = bindings.call_name(&mut store)?;
        let wit_caps = bindings.call_required_capabilities(&mut store)?;
        let actions = bindings.call_actions(&mut store)?;
        let file_types = bindings.call_file_types(&mut store)?;

        let capabilities = wit_caps.into_iter().map(wit_cap_to_capability).collect();

        Ok(Self {
            engine,
            component,
            limits,
            cached_name: name,
            cached_capabilities: capabilities,
            cached_actions: actions,
            cached_file_types: file_types,
        })
    }

    /// Execute the WASM component with a fresh Store.
    fn run_execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        let mut linker = Linker::new(&self.engine);
        SkillPlugin::add_to_linker::<PluginState, HasSelf<PluginState>>(
            &mut linker,
            |state| state,
        )?;

        let store_limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .instances(self.limits.max_instances)
            .build();
        let state = PluginState {
            db: ctx.db.clone(),
            limits: store_limits,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(self.limits.fuel)?;

        let bindings = SkillPlugin::instantiate(&mut store, &self.component, &linker)?;

        // Convert SkillDocument to WIT type (body only)
        let wit_doc = crate::wasm::host_bridge::sovereign::skill::types::SkillDocument {
            id: doc.id.clone(),
            title: doc.title.clone(),
            body: doc.content.body.clone(),
        };

        // Convert granted capabilities to WIT enum
        let wit_caps: Vec<_> = ctx.granted.iter().map(capability_to_wit).collect();

        let result = bindings.call_execute(&mut store, action, &wit_doc, params, &wit_caps)?;

        match result {
            Ok(output) => Ok(wit_output_to_skill_output(output)),
            Err(msg) => anyhow::bail!("WASM skill error: {msg}"),
        }
    }
}

impl CoreSkill for WasmSkill {
    fn name(&self) -> &str {
        &self.cached_name
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        self.cached_capabilities.clone()
    }

    fn activate(&mut self) -> anyhow::Result<()> {
        Ok(()) // WASM components are stateless
    }

    fn deactivate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        self.run_execute(action, doc, params, ctx)
    }

    fn actions(&self) -> Vec<(String, String)> {
        self.cached_actions.clone()
    }

    fn file_types(&self) -> Vec<String> {
        self.cached_file_types.clone()
    }
}

// --- Conversion helpers ---

fn wit_cap_to_capability(
    c: crate::wasm::host_bridge::sovereign::skill::types::Capability,
) -> Capability {
    use crate::wasm::host_bridge::sovereign::skill::types::Capability as WitCap;
    match c {
        WitCap::ReadDocument => Capability::ReadDocument,
        WitCap::WriteDocument => Capability::WriteDocument,
        WitCap::ReadAllDocuments => Capability::ReadAllDocuments,
        WitCap::WriteAllDocuments => Capability::WriteAllDocuments,
        WitCap::ReadFilesystem => Capability::ReadFilesystem,
        WitCap::WriteFilesystem => Capability::WriteFilesystem,
        WitCap::Network => Capability::Network,
    }
}

fn capability_to_wit(
    c: &Capability,
) -> crate::wasm::host_bridge::sovereign::skill::types::Capability {
    use crate::wasm::host_bridge::sovereign::skill::types::Capability as WitCap;
    match c {
        Capability::ReadDocument => WitCap::ReadDocument,
        Capability::WriteDocument => WitCap::WriteDocument,
        Capability::ReadAllDocuments => WitCap::ReadAllDocuments,
        Capability::WriteAllDocuments => WitCap::WriteAllDocuments,
        Capability::ReadFilesystem => WitCap::ReadFilesystem,
        Capability::WriteFilesystem => WitCap::WriteFilesystem,
        Capability::Network => WitCap::Network,
    }
}

fn wit_output_to_skill_output(
    output: crate::wasm::host_bridge::sovereign::skill::types::SkillOutput,
) -> SkillOutput {
    use crate::wasm::host_bridge::sovereign::skill::types::SkillOutput as WitOutput;
    match output {
        WitOutput::ContentUpdate(body) => {
            SkillOutput::ContentUpdate(ContentFields {
                body,
                ..Default::default()
            })
        }
        WitOutput::File(f) => SkillOutput::File {
            name: f.name,
            mime_type: f.mime_type,
            data: f.data,
        },
        WitOutput::None => SkillOutput::None,
        WitOutput::StructuredData(sd) => SkillOutput::StructuredData {
            kind: sd.kind,
            json: sd.json,
        },
    }
}

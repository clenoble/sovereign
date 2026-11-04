//! The shell's wgpu instance, built in one place (v0.0.12, no OpenGL backend
//! on Windows, `doc/plans/V0012-no-gl-windows.md`, W1–W4).
//!
//! `vello::util::RenderContext::new()` asks wgpu for every backend, so on
//! Windows wgpu-hal created a WGL instance (a hidden GL context) next to the
//! DX12 or Vulkan adapter the shell actually uses. Here the descriptor is
//! vello 0.9.0's, with only the backends changed.

use vello::util::RenderContext;
use vello::wgpu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    Other,
}

/// `Platform::Windows` under `cfg(windows)`, `Platform::Other` otherwise.
pub(crate) fn current_platform() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Other
    }
}

/// The backends the shell asks wgpu for. A set `WGPU_BACKEND` decides, even
/// empty or unknown, parsed as wgpu parses it (W3, W3b); unset, Windows gets
/// DX12 and Vulkan only (W1), other platforms wgpu's default, GL included (W2).
pub(crate) fn shell_backends(platform: Platform, wgpu_backend_env: Option<&str>) -> wgpu::Backends {
    match (wgpu_backend_env, platform) {
        (Some(v), _) => wgpu::Backends::from_comma_list(v),
        (None, Platform::Windows) => wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        (None, Platform::Other) => wgpu::Backends::default(),
    }
}

/// What `RenderContext::new()` (vello 0.9.0, `src/util.rs`) passes to
/// `Instance::new`, with `backends` from `shell_backends` (W1, W2).
pub(crate) fn instance_descriptor(platform: Platform, wgpu_backend_env: Option<&str>) -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor {
        display: None,
        backends: shell_backends(platform, wgpu_backend_env),
        flags: wgpu::InstanceFlags::from_build_config().with_env(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::from_env_or_default(),
    }
}

/// The one place the shell builds its `RenderContext` (W4). `WGPU_BACKEND` is
/// read as wgpu reads it: a value that is not UTF-8 counts as unset.
pub(crate) fn render_context() -> RenderContext {
    let env = std::env::var("WGPU_BACKEND").ok();
    RenderContext {
        instance: wgpu::Instance::new(instance_descriptor(current_platform(), env.as_deref())),
        devices: Vec::new(),
    }
}

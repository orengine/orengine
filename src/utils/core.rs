/// ID of the CPU core.
pub type CoreId = core_affinity::CoreId;

/// Returns the list of available CPU cores.
pub fn get_core_ids() -> Option<Vec<CoreId>> {
    core_affinity::get_core_ids()
}

/// Sets the affinity of the current thread to the given CPU core.
pub fn set_for_current(core_id: CoreId) {
    core_affinity::set_for_current(core_id);
}
